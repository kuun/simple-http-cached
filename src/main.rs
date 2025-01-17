#![deny(warnings)]

use std::net::SocketAddr;

use bytes::Bytes;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::{body::Incoming, Request, Response, StatusCode};
use hyper_util::client::legacy::{Client, Error};

use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::net::TcpListener;

// To try this example:
// 1. cargo run --example http_proxy
// 2. config http_proxy in command line
//    $ export http_proxy=http://127.0.0.1:8100
//    $ export https_proxy=http://127.0.0.1:8100
// 3. send requests
//    $ curl -i https://www.some_domain.com/
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = SocketAddr::from(([127, 0, 0, 1], 8100));

    let listener = TcpListener::bind(addr).await?;
    println!("Listening on http://{}", addr);

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);

        tokio::task::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .preserve_header_case(true)
                .title_case_headers(true)
                .serve_connection(io, service_fn(proxy))
                .await
            {
                println!("Failed to serve connection: {:?}", err);
            }
        });
    }
}

async fn proxy(
    req: Request<Incoming>,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    let host = "snapshot.debian.org";
    let uri = req.uri().clone();
 /*    let uri = req.uri().clone();
    let (parts, body) = req.into_parts();
    let mut req = Request::from_parts(parts, body.boxed());
    req.headers_mut()
        .insert("Host", HeaderValue::from_static(host)); */

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
            Ok(resp.map(|b| b.boxed()))
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
