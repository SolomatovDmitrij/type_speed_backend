use actix_web::{web, http, FromRequest, middleware, HttpResponse, App, HttpRequest, HttpServer, Error };

use actix_cors::{Cors};
use env_logger::Env;

use serde_derive::{Deserialize, Serialize};
use actix_service::Service;
use futures::future::FutureExt;

use mongodb::{Client, options::ClientOptions, options::FindOptions, Database};
use bson::{doc, Document, oid::ObjectId, Bson };

use futures::stream::StreamExt;
//параметры запроса для загрузки глобального текста
#[derive(Deserialize, Debug)]
struct FormData {
    new_text: String,
}
//параметры запроса для сохранения результата в базу 
#[derive(Deserialize, Debug)]
struct ResultData {
    speed: u32,
    error: u32,
    user_id: String,
    text_id: String,
}
#[derive(Deserialize, Debug, Serialize)]
struct User {
    id: String,
    name: String,
    password: String,
}

//полчить кусочек текста
async fn get_text(req: HttpRequest) -> Result<HttpResponse, Error> {
    let db = connect_to_mongodb().await;
    let collection = db.collection("texts");
    let mut cursor = collection.find_one(None, None).await.expect("не удалось получить объект");
    if let Some(result) = cursor {
        return HttpResponse::Ok().json(result).await;
    }

    HttpResponse::Ok().body("Some text for type speed").await
}

//загрузить текст в базу и разбить его не кусочки для набора
async fn load_text(form: web::Json<FormData>) -> HttpResponse {

    let db = connect_to_mongodb().await;
    let collection = db.collection("texts");
    let text_slice: &str = &form.new_text;
    let text_vec: Vec<String> = text_slice.chars().collect::<Vec<_>>().chunks(1000)
        .map(|chunk| chunk.iter().collect::<String>()).collect();
    println!("text: {:?}", text_vec);

    let text_docs: Vec<Document> = text_vec.into_iter().map(|elem| doc! {"text": elem}).collect();
    collection.insert_many(text_docs, None).await.expect("не удалось записать текст в базу данных");
//    println!("docs: {:?}", text_docs);

    HttpResponse::Ok().body("ok")

}

//подключение к базе данных
async fn connect_to_mongodb() -> Database {
    let mut client_options =
        ClientOptions::parse("mongodb+srv://admin:admin1982@cluster0-mjnlg.mongodb.net/test?retryWrites=true&w=majority").await.expect("не удалось подключиться к базе данных");

    let client = Client::with_options(client_options).expect("не удалось подключиться к базе данных");
    let db = client.database("speed_type");

    db
}

//сохранить результат в базу
async fn save_result_get(req: web::Query<ResultData>) -> HttpResponse {

    println!("save result: {:?}", req);

    let db=connect_to_mongodb().await;
    let collection = db.collection("results");
    let collection_text = db.collection("texts");
    let res = collection.insert_one(doc! {
        "speed": req.speed,
        "error_count": req.error,
        "user_id": &req.user_id,
    }, None).await;
    let res_delete = bson::oid::ObjectId::with_string(&req.text_id);
    println!("delete: {:?}", res_delete);
    
    if let Ok(obj_id) = res_delete {
        let res = collection_text.delete_one( doc! { "_id": obj_id }, None ).await.expect("delete error");
        println!("delete text from base: {:?}", res);
    }
    

    HttpResponse::Ok().body("ok")
}

//получение результатов
async fn load_result(user: web::Json<User>) -> Result<HttpResponse, Error> {

    let db = connect_to_mongodb().await;
    let collection = db.collection("results");
//    let find_options = FindOptions::builder().sort(doc! { "speed": 1 }).build();
    let filter = doc! { "user_id": &user.id };
    let mut cursor = collection.find(filter, None).await.unwrap();

    let mut results = vec![];
    while let Some(doc) = cursor.next().await {
        results.push(doc.unwrap());
    }

    Ok(HttpResponse::Ok().json(results))
}

//логирование пользователя, если не находится в базе, то создается новый и возвращается его id
async fn login(mut user: web::Json<User>) -> Result<HttpResponse, Error> {
    let db = connect_to_mongodb().await;
    let collection = db.collection("users");
    
    let filter = doc! { "username": &user.name }; //, "password": &user.password };
    if let Ok(result_document) = collection.find_one(filter, None).await {
        if let Some(user_doc) = result_document {
            if let Some(&Bson::String(ref password)) = user_doc.get("password") {
                if password == &user.password {
                    user.id = user_doc.get_object_id("_id").unwrap().to_hex();
                }
            }
        } else {
            let res = collection.insert_one(doc! {
                "username": &user.name,
                "password": &user.password,
            }, None).await;
            if let Bson::ObjectId(mongo_id) = res.unwrap().inserted_id {
                user.id = mongo_id.to_hex();
            };
        }
    };

    if user.id == "" { Ok(HttpResponse::Unauthorized().finish()) } else {
        Ok(HttpResponse::Ok().json(User { 
            id: user.id.clone(),
            name: user.name.clone(),
            password: user.password.clone(),
        })) 
    }

}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    
    env_logger::from_env(Env::default().default_filter_or("info")).init();

    HttpServer::new(|| {
        App::new()
            .wrap(middleware::Logger::default())
            .wrap(
                Cors::new() // <- Construct CORS middleware builder
//                .allowed_origin("http://192.168.0.210:3001")
//                .allowed_methods(vec!["GET", "POST"])
//                .allowed_headers(vec![http::header::AUTHORIZATION, http::header::ACCEPT])
//                .allowed_header(http::header::CONTENT_TYPE)
//                .max_age(3600)
                .send_wildcard()
                .finish())
            
            .app_data(web::Json::<FormData>::configure(|cfg| {
                cfg.limit(4096000000)
            }))
            .app_data(web::PayloadConfig::new(1_000_000 * 250))
            .route("/text", web::get().to(get_text))
            .route("/load_text", web::post().to(load_text))
            //.route("/save_result_post", web::post().to(save_result_post))
            .route("/save_result_get", web::get().to(save_result_get))
            .route("/load_result", web::post().to(load_result))
            .route("/api/login", web::post().to(login))
    })
  //  .bind("192.168.0.210:3004")?
    .bind("147.78.66.199:3004")?
    .run()
    .await

}
