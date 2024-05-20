use std::{env, thread};
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::net::TcpListener;
use std::path::Path;


use http::{Method, Request};
use crate::http::Code;

mod http;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    let args_str = args
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<&str>>();
    let serving_folder = match args_str.as_slice() {
        ["--directory", directory] => directory,
        _ => ".",
    };

    println!("serving and uploading folder: {:?}", serving_folder);

    let listener = TcpListener::bind("127.0.0.1:4221").unwrap();

    for stream in listener.incoming() {
        let serving_folder = serving_folder.to_string();
        thread::spawn(move || {
            match stream {
                Ok(_stream) => {
                    if let Ok(mut req) = Request::new(_stream.try_clone().unwrap()) {
                        println!("accepted {} {:?}", req.uri, req);

                        let mut response = match (req.method, req.path.iter().map(|s| s.as_str()).collect::<Vec<&str>>().as_slice()) {
                            (Method::GET, ["", ""]) =>
                                req.response(),
                            (Method::GET, ["", "user-agent"]) =>
                                {
                                    let user_agent = req.get_header("user-agent").map(|s| {
                                        s.clone()
                                    }).unwrap_or("".to_string());
                                    req.response().with_header("Content-Type", "text/plain").with_content(&user_agent)
                                }
                            (Method::GET, ["", "files", filename]) => {
                                let local_file = Path::new(serving_folder.as_str()).join(filename);
                                if let Ok(f) = File::open(&local_file) {
                                    let size = f.metadata().unwrap().len();
                                    let reader = BufReader::new(f);
                                    req.response()
                                        .with_header("Content-Type", "application/octet-stream")
                                        .with_content_reader(reader, size)
                                } else {
                                    req.response().with_code(Code::NotFound)
                                }
                            }
                            (Method::POST, ["", "files", filename]) => {
                                let local_file = Path::new(serving_folder.as_str()).join(filename);
                                let content_length = req.get_header("content-length").map(|s| {
                                    s.parse::<u32>()
                                }).unwrap().ok();

                                let f = File::create(local_file).unwrap();
                                let f = BufWriter::new(f);
                                req.write_content(f, content_length).unwrap();
                                req
                                    .response()
                                    .with_code(Code::Created)
                            }
                            (Method::GET, ["", "echo", rest @ .. ]) =>
                                {
                                    let content = rest.join("/");
                                    req.response().with_header("Content-Type", "text/plain")
                                        .with_content(&content)
                                }
                            _ =>
                                req.response().with_code(Code::NotFound),
                        };
                        response.flush().unwrap_or(());
                    } else {
                        println!("request failed to parse");
                    };
                }
                Err(e) => {
                    println!("error: {}", e);
                }
            }
        });
    }
}
