use crate::utilities::token_validation::validate_cognito_token;
use crate::utilities::config;
use crate::models::errors::AuthorizationError;

/// A reusable function that validates the access token and extracts the user ID before executing an action.
/// This ensures authentication is enforced consistently across endpoints.
pub async fn with_valid_user<F, Fut, R, E>(
    token: &str,
    action: F
) -> Result<R, E>
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = Result<R, E>>,
    E: From<AuthorizationError>,
{
    let user_pool_id = config::get_user_pool_id();
    let region = config::get_aws_region();

    let claims = validate_cognito_token(token, &user_pool_id, &region).await;

    match claims {
        Ok(claims) => {
            let user_id = claims.username.clone();
            action(user_id).await
        }
        Err(e) => Err(E::from(AuthorizationError::Unauthorized(format!("{:?}", e)))),
    }
}
