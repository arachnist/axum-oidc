use axum::{
    error_handling::HandleErrorLayer,
    http::Uri,
    response::IntoResponse,
    routing::{any, get},
    Router,
};
use axum_oidc::{
    error::MiddlewareError, handle_oidc_redirect, EmptyAdditionalClaims, OidcAuthLayer, OidcClaims,
    OidcClient, OidcLoginLayer, OidcRpInitiatedLogout,
};
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_sessions::{
    cookie::{time::Duration, SameSite},
    Expiry, MemoryStore, SessionManagerLayer,
};

pub async fn run(issuer: String, client_id: String, client_secret: Option<String>) {
    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false)
        .with_same_site(SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(Duration::seconds(120)));

    let oidc_login_service = ServiceBuilder::new()
        .layer(HandleErrorLayer::new(|e: MiddlewareError| async {
            dbg!(&e);
            e.into_response()
        }))
        .layer(OidcLoginLayer::<EmptyAdditionalClaims>::new());

    let mut oidc_client = OidcClient::<EmptyAdditionalClaims>::builder()
        .with_default_http_client()
        .with_redirect_url(Uri::from_static("http://localhost:8080/oidc"))
        .with_client_id(client_id);
    if let Some(client_secret) = client_secret {
        oidc_client = oidc_client.with_client_secret(client_secret);
    }
    let oidc_client = oidc_client.discover(issuer).await.unwrap().build();

    let oidc_auth_service = ServiceBuilder::new()
        .layer(HandleErrorLayer::new(|e: MiddlewareError| async {
            dbg!(&e);
            e.into_response()
        }))
        .layer(OidcAuthLayer::new(oidc_client));

    let app = Router::new()
        .route("/foo", get(authenticated))
        .route("/logout", get(logout))
        .layer(oidc_login_service)
        .route("/bar", get(maybe_authenticated))
        .route("/oidc", any(handle_oidc_redirect::<EmptyAdditionalClaims>))
        .layer(oidc_auth_service)
        .layer(session_layer);

    let listener = TcpListener::bind("[::]:8080").await.unwrap();
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}

async fn authenticated(claims: OidcClaims<EmptyAdditionalClaims>) -> impl IntoResponse {
    format!("Hello {}", claims.subject().as_str())
}

#[axum::debug_handler]
async fn maybe_authenticated(
    claims: Result<OidcClaims<EmptyAdditionalClaims>, axum_oidc::error::ExtractorError>,
) -> impl IntoResponse {
    if let Ok(claims) = claims {
        format!(
            "Hello {}! You are already logged in from another Handler.",
            claims.subject().as_str()
        )
    } else {
        "Hello anon!".to_string()
    }
}

async fn logout(logout: OidcRpInitiatedLogout) -> impl IntoResponse {
    logout.with_post_logout_redirect(Uri::from_static("https://example.com"))
}
