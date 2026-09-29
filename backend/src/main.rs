//! Production entry point (Heroku container): Axum on $PORT.
use std::net::SocketAddr;
use std::time::Duration;

use altim::app::{AppState, data::warm_selections, router};
use altim::auth::Auth;
use altim::live::LiveHub;

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(3000);
    // A wrong ALTIM_* configuration stops the server at start-up (never a half-open private access).
    let auth = match Auth::from_env() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let locked = auth.config().is_none();
    let live = LiveHub::new();
    let app = router(AppState::new(live.clone()), auth);
    // Without a login (explicit local development mode), only this machine can reach the server.
    let host = if altim::auth::dev_open() { "127.0.0.1" } else { "0.0.0.0" };
    if host == "127.0.0.1" {
        eprintln!("ALTIM_DEV_OPEN=1 : accès sans connexion, limité à cette machine (127.0.0.1).");
    } else if locked {
        eprintln!("ALTIM_USER, ALTIM_PASSWORD_HASH et ALTIM_SESSION_SECRET absents : tout accès est refusé.");
    }
    let listener = tokio::net::TcpListener::bind((host, port)).await.expect("port occupé");
    println!("Altim web en ligne sur http://localhost:{port}");
    let production = std::env::var("NODE_ENV").is_ok_and(|v| v == "production") || std::env::var("DYNO").is_ok_and(|v| !v.is_empty());
    if production {
        warm_selections();
    }
    // EUR/USD rate of the euro texts, read before the first visitor asks.
    tokio::spawn(async {
        altim::fx::ensure().await;
    });
    // Heroku restarts every dyno at least once a day with SIGTERM, then kills it 30 s later: new requests are refused,
    // live streams are closed (the apps and the browser reconnect at once to the new dyno) and the exchanges' sockets
    // are shut before leaving.
    let shutdown = async move {
        let term = async {
            #[cfg(unix)]
            {
                let mut s = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("SIGTERM");
                s.recv().await;
                "SIGTERM"
            }
            #[cfg(not(unix))]
            std::future::pending::<&str>().await
        };
        let name = tokio::select! {
            n = term => n,
            _ = tokio::signal::ctrl_c() => "SIGINT",
        };
        println!("{name} : arrêt propre");
        live.close();
        // Open streams would keep the server alive: 3 s of grace, well inside Heroku's 30 s.
        tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(3)).await;
            std::process::exit(0);
        });
    };
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).with_graceful_shutdown(shutdown).await.expect("serveur");
}
