use salvo::prelude::*;

#[handler]
async fn index(req: &mut Request, res: &mut Response) {
    res.render(Text::Plain(format!("remote address: {:?}", req.remote_addr())));
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().get(index);

    let acceptor = TcpListener::new("127.0.0.1:7878").bind().await;
    Server::new(acceptor).serve(router).await;
}
