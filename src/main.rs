#![deny(warnings)]

use clap::Parser;
use bytes::Bytes;
use futures_util::TryStreamExt;
use http_body_util::Empty;
use http_body_util::{combinators::BoxBody, BodyExt, Full, StreamBody};
use hyper::Uri;
use hyper::{
    body::{Frame, Incoming},
    Request, Response, StatusCode,
};
use hyper_util::client::legacy::{Client, Error};
use std::io::Write;
use std::sync::Arc;
use std::{convert::Infallible, net::SocketAddr};

use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::net::TcpListener;
use tokio_util::io::{InspectReader, ReaderStream, StreamReader};

/// Simple HTTP Proxy with Caching
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Target server to proxy requests to
    #[clap(short, long, default_value = "snapshot.debian.org")]
    target_server: String,

    /// Address to listen on
    #[clap(short, long, default_value = "127.0.0.1:8100")]
    listen_addr: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let addr : SocketAddr = args.listen_addr.parse()?;

    let listener = TcpListener::bind(addr).await?;
    println!("Listening on http://{}", addr);

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let target_server = args.target_server.clone();

        tokio::task::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .preserve_header_case(true)
                .title_case_headers(true)
                .serve_connection(io, 
                    service_fn(|req| proxy(req, target_server.clone())))
                .await
            {
                println!("Failed to serve connection: {:?}", err);
            }
        });
    }
}

async fn proxy(
    req: Request<Incoming>,
    target_server : String,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, Infallible> {
    let host = target_server;
    let uri = req.uri().clone();

    println!("{:?} -->", uri);
    if let Some(resp) = get_cached_response(&req).await {
        return Ok(resp);
    }

    let https = hyper_rustls::HttpsConnectorBuilder::new()
        .with_native_roots()
        .expect("no native root CA certificates found")
        .https_only()
        .enable_http1()
        .build();

    let client: Client<_, BoxBody<Bytes, hyper::Error>> =
        Client::builder(TokioExecutor::new()).build(https);
    let new_req = Request::builder()
        .method(req.method())
        .uri(format!("https://{}{}", host, uri))
        .body(req.boxed())
        .unwrap();

    let resp = client.request(new_req).await;
    match resp {
        Ok(resp) => {
            println!("{:?} --> {:?}", uri, resp.status());

            // Handle redirection by returning the response to the client
            if resp.status().is_redirection() {
                if let Some(location) = resp.headers().get("location") {
                    println!("Redirecting to {:?}", location);
                    let location_str = location.to_str().unwrap();
                    let redirect_cache_file_path = format!("{}_redirect", path_from_uri(&uri));
                    tokio::fs::write(&redirect_cache_file_path, location_str)
                        .await
                        .unwrap();
                    return Ok(resp.map(|body| BoxBody::new(body)));
                }
            }

            // open file to save response body

            let cache_file_path = path_from_uri(&uri);
            let file = Arc::new(std::fs::File::create(cache_file_path).unwrap());

            let stream = resp.into_data_stream();

            let reader = StreamReader::new(
                stream.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
            );
            let reader = InspectReader::new(reader, move |chunk| {
                // println!("{} bytes read", chunk.len());
                let mut file = Arc::clone(&file);

                if chunk.len() > 0 {
                    file.write_all(chunk).unwrap();
                } else {
                    // close file
                    file.flush().unwrap();
                }
            });
            let stream = ReaderStream::new(reader)
                .map_ok(|data| Frame::data(data))
                .map_err(|e| panic!("Error reading response: {:?}", e));
            let body = StreamBody::new(stream);
            let body = BoxBody::new(body);
            let resp = Response::new(body);
            Ok(resp)
        }
        Err(e) => {
            println!("{:?} --> {:?}", uri, e);
            let mut internal_error = Response::new(error_body(e));
            *internal_error.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            Ok(internal_error)
        }
    }
}

fn error_body(e: Error) -> BoxBody<Bytes, hyper::Error> {
    Full::<Bytes>::new(format!("Error: {}", e).into())
        .map_err(|never| match never {})
        .boxed()
}

// implement a function to handle the request, cache the response and return it
// if the request is already cached
async fn get_cached_response(
    req: &Request<Incoming>,
) -> Option<Response<BoxBody<Bytes, hyper::Error>>> {
    let uri = req.uri().clone();
    let cache_file_path = path_from_uri(&uri);

    // Check if the redirect cache file exists
    let redirect_cache_file_path = format!("{}_redirect", cache_file_path);
    if let Ok(location) = std::fs::read_to_string(&redirect_cache_file_path) {
        let response = Response::builder()
            .status(StatusCode::FOUND)
            .header("Location", location)
            .body(Empty::<Bytes>::new().map_err(|e| match e {}).boxed())
            .unwrap();
        return Some(response);
    }

    // Check if the cache file exists
    let file = tokio::fs::File::open(&cache_file_path).await.ok()?;
    let stream = ReaderStream::new(file)
        .map_ok(|data| Frame::data(data))
        .map_err(|e| panic!("Error reading cache file: {:?}", e));
    let body = StreamBody::new(stream);
    let body = BoxBody::new(body);
    Some(Response::new(body))
}

fn path_from_uri(uri: &Uri) -> String {
    let cache_dir = "/var/lib/simple_http_cache";
    let path = uri
        .to_string()
        .replace("/", "_")
        .replace("?", "_")
        .replace("&", "_")
        .replace("=", "_");
    let file_path = format!("{}/{}", cache_dir, path);
    println!("File path: {}", file_path);
    file_path
}
