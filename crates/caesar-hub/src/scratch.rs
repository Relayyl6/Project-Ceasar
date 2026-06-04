use tokio::io::{AsyncReadExt, AsyncBufReadExt, BufReader};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    let mut reader = BufReader::new(tokio::io::empty());
    let mut buf = String::new();
    let mut take = (&mut reader).take(1024);
    let n = take.read_line(&mut buf).await.unwrap();
    println!("n={}", n);
}
