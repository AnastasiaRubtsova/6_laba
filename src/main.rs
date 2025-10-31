use serde::{Serialize, Deserialize};
use postgres::{Client, NoTls, Error as PostgresError};
use std::env;
use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write};
use serde_json;

#[derive(Serialize, Deserialize)]
struct User {
    id: Option<i32>,
    name: String,
    email: String,
}

const OK_RESPONSE: &str = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n";
const NOT_FOUND: &str = "HTTP/1.1 404 NOT FOUND\r\n\r\n";
const INTERNAL_ERROR: &str = "HTTP/1.1 500 INTERNAL ERROR\r\n\r\n";

fn main() -> Result<(), PostgresError> {
    // Читаем DATABASE_URL из окружения во время выполнения
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    set_database(&db_url)?;

    let listener = TcpListener::bind("0.0.0.0:8080").unwrap();
    println!("Server listening on port 8080");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                handle_client(stream, &db_url);
            }
            Err(e) => eprintln!("Unable to accept connection: {}", e),
        }
    }
    Ok(())
}

fn handle_client(mut stream: TcpStream, db_url: &str) {
    let mut buffer = [0; 1024];
    let mut request = String::new();

    match stream.read(&mut buffer) {
        Ok(size) if size > 0 => {
            request.push_str(&String::from_utf8_lossy(&buffer[..size]));

            let (status_line, content) = match &*request {
                r if r.starts_with("POST /users") => handle_post_request(r, db_url),
                r if r.starts_with("GET /users/") => handle_get_request(r, db_url),
                r if r.starts_with("GET /users") => handle_get_all_request(r, db_url),
                r if r.starts_with("PUT /users/") => handle_put_request(r, db_url),
                r if r.starts_with("DELETE /users/") => handle_delete_request(r, db_url),
                _ => (NOT_FOUND.to_string(), "404 Not Found".to_string()),
            };

            let response = format!("{}{}", status_line, content);
            if let Err(e) = stream.write_all(response.as_bytes()) {
                eprintln!("Failed to write response: {}", e);
            }
        }
        Ok(_) => eprintln!("Received empty request"),
        Err(e) => eprintln!("Unable to read stream: {}", e),
    }
}

fn handle_post_request(request: &str, db_url: &str) -> (String, String) {
    match get_user_request_body(request) {
        Ok(user) => {
            match Client::connect(db_url, NoTls) {
                Ok(mut client) => {
                    let res = client.execute(
                        "INSERT INTO users (name, email) VALUES ($1, $2)",
                        &[&user.name, &user.email],
                    );
                    match res {
                        Ok(_) => (OK_RESPONSE.to_string(), "User created".to_string()),
                        Err(_) => (INTERNAL_ERROR.to_string(), "Internal error".to_string()),
                    }
                }
                Err(_) => (INTERNAL_ERROR.to_string(), "Internal error".to_string()),
            }
        }
        Err(_) => (INTERNAL_ERROR.to_string(), "Invalid JSON".to_string()),
    }
}

fn handle_get_request(request: &str, db_url: &str) -> (String, String) {
    let id_opt = get_id(request).parse::<i32>().ok();
    if id_opt.is_none() {
        return (NOT_FOUND.to_string(), "Invalid ID".to_string());
    }
    let id = id_opt.unwrap();

    match Client::connect(db_url, NoTls) {
        Ok(mut client) => {
            let row = client.query_opt("SELECT id, name, email FROM users WHERE id = $1", &[&id]);
            match row {
                Ok(Some(row)) => {
                    let user = User {
                        id: row.get(0),
                        name: row.get(1),
                        email: row.get(2),
                    };
                    (OK_RESPONSE.to_string(), serde_json::to_string(&user).unwrap())
                }
                Ok(None) => (NOT_FOUND.to_string(), "User not found".to_string()),
                Err(_) => (INTERNAL_ERROR.to_string(), "Internal error".to_string()),
            }
        }
        Err(_) => (INTERNAL_ERROR.to_string(), "Internal error".to_string()),
    }
}

fn handle_get_all_request(_request: &str, db_url: &str) -> (String, String) {
    match Client::connect(db_url, NoTls) {
        Ok(mut client) => {
            let rows = client.query("SELECT id, name, email FROM users", &[]).unwrap_or_default();
            let users: Vec<User> = rows
                .iter()
                .map(|row| User {
                    id: row.get(0),
                    name: row.get(1),
                    email: row.get(2),
                })
                .collect();
            (OK_RESPONSE.to_string(), serde_json::to_string(&users).unwrap())
        }
        Err(_) => (INTERNAL_ERROR.to_string(), "Internal error".to_string()),
    }
}

fn handle_put_request(request: &str, db_url: &str) -> (String, String) {
    let id_opt = get_id(request).parse::<i32>().ok();
    if id_opt.is_none() {
        return (NOT_FOUND.to_string(), "Invalid ID".to_string());
    }
    let id = id_opt.unwrap();

    match get_user_request_body(request) {
        Ok(user) => {
            match Client::connect(db_url, NoTls) {
                Ok(mut client) => {
                    let res = client.execute(
                        "UPDATE users SET name = $1, email = $2 WHERE id = $3",
                        &[&user.name, &user.email, &id],
                    );
                    match res {
                        Ok(affected) if affected > 0 => (OK_RESPONSE.to_string(), "User updated".to_string()),
                        Ok(_) => (NOT_FOUND.to_string(), "User not found".to_string()),
                        Err(_) => (INTERNAL_ERROR.to_string(), "Internal error".to_string()),
                    }
                }
                Err(_) => (INTERNAL_ERROR.to_string(), "Internal error".to_string()),
            }
        }
        Err(_) => (INTERNAL_ERROR.to_string(), "Invalid JSON".to_string()),
    }
}

fn handle_delete_request(request: &str, db_url: &str) -> (String, String) {
    let id_opt = get_id(request).parse::<i32>().ok();
    if id_opt.is_none() {
        return (NOT_FOUND.to_string(), "Invalid ID".to_string());
    }
    let id = id_opt.unwrap();

    match Client::connect(db_url, NoTls) {
        Ok(mut client) => {
            let res = client.execute("DELETE FROM users WHERE id = $1", &[&id]);
            match res {
                Ok(affected) if affected > 0 => (OK_RESPONSE.to_string(), "User deleted".to_string()),
                Ok(_) => (NOT_FOUND.to_string(), "User not found".to_string()),
                Err(_) => (INTERNAL_ERROR.to_string(), "Internal error".to_string()),
            }
        }
        Err(_) => (INTERNAL_ERROR.to_string(), "Internal error".to_string()),
    }
}

fn set_database(db_url: &str) -> Result<(), PostgresError> {
    let mut client = Client::connect(db_url, NoTls)?;
    client.batch_execute(
        "
        CREATE TABLE IF NOT EXISTS users (
            id SERIAL PRIMARY KEY,
            name VARCHAR NOT NULL,
            email VARCHAR NOT NULL
        )
    ",
    )?;
    Ok(())
}

fn get_id(request: &str) -> &str {
    request.split("/").nth(2).unwrap_or_default().split_whitespace().next().unwrap_or_default()
}

fn get_user_request_body(request: &str) -> Result<User, serde_json::Error> {
    serde_json::from_str(request.split("\r\n\r\n").last().unwrap_or_default())
}

