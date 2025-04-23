use aws_config::meta::region::RegionProviderChain;
use aws_sdk_sts::Client as StsClient;
use aws_sdk_cognitoidentityprovider::Client as CognitoClient;
use aws_sdk_dynamodb::Client as DynamoDbClient;
use aws_sdk_sts::config::Credentials;
use aws_sdk_sts::types::Credentials as StsCredentials;
use aws_config::sts::AssumeRoleProvider;
use aws_sdk_sqs::Client as SqsClient;
use aws_sdk_cloudwatch::{Client as CloudWatchClient};
use aws_sdk_secretsmanager::Client as SecretsManagerClient;
use once_cell::sync::OnceCell;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

const ROLE_ARN: &str = "arn:aws:iam::971422686568:role/Foxy-dev-Lambda-ExecutionRole";
const SQS_ROLE_ARN: &str = "arn:aws:iam::971422686568:role/Foxy-dev-FoxyLambdaSQSRole";

async fn assume_role(arn: &str) -> Result<StsCredentials, Box<dyn std::error::Error>> {
    let region_provider = RegionProviderChain::default_provider();
    let shared_config = aws_config::from_env().region(region_provider).load().await;
    let sts_client = StsClient::new(&shared_config);

    let assumed_role = sts_client.assume_role()
        .role_arn(arn)
        .role_session_name("IntegrationTestSession")
        .send()
        .await?;

    assumed_role.credentials.ok_or_else(|| "No credentials returned".into())
}

pub async fn get_cognito_client_with_assumed_role() -> Result<CognitoClient, Box<dyn std::error::Error>> {
    let creds = assume_role(ROLE_ARN).await?;
    let region_provider = RegionProviderChain::default_provider();

    // Directly convert &str to String without `ok_or_else`
    let access_key = creds.access_key_id().to_string();
    let secret_key = creds.secret_access_key().to_string();
    let session_token = creds.session_token().to_string();

    let shared_config = aws_config::from_env()
        .region(region_provider)
        .credentials_provider(Credentials::new(
            access_key,
            secret_key,
            Some(session_token),
            None,
            "CognitoAssumedRole",
        ))
        .load()
        .await;

    Ok(CognitoClient::new(&shared_config))
}


pub async fn get_dynamodb_client_with_assumed_role() -> DynamoDbClient {
    let provider = AssumeRoleProvider::builder(ROLE_ARN)
        .session_name("foxy-test-session")
        .build()
        .await;

    let config = aws_config::from_env()
        .credentials_provider(provider)
        .load()
        .await;

    DynamoDbClient::new(&config)
}


pub async fn get_sqs_client_with_assumed_role() -> Result<SqsClient, Box<dyn std::error::Error>> {
    let creds = assume_role(SQS_ROLE_ARN).await?;
    let region_provider = RegionProviderChain::default_provider().or_else("eu-north-1");

    let access_key = creds.access_key_id().to_string();
    let secret_key = creds.secret_access_key().to_string();
    let session_token = creds.session_token().to_string();

    let shared_config = aws_config::from_env()
        .region(region_provider)
        .credentials_provider(Credentials::new(
            access_key,
            secret_key,
            Some(session_token),
            None,
            "SqsAssumedRole",
        ))
        .load()
        .await;

    Ok(SqsClient::new(&shared_config))
}

pub async fn get_cloudwatch_client_with_assumed_role() -> Result<CloudWatchClient, Box<dyn std::error::Error>> {
    let creds = assume_role(ROLE_ARN).await?;
    let region_provider = RegionProviderChain::default_provider();

    // Directly convert &str to String without `ok_or_else`
    let access_key = creds.access_key_id().to_string();
    let secret_key = creds.secret_access_key().to_string();
    let session_token = creds.session_token().to_string();

    let shared_config = aws_config::from_env()
        .region(region_provider)
        .credentials_provider(Credentials::new(
            access_key,
            secret_key,
            Some(session_token),
            None,
            "CloudwatchAssumedRole",
        ))
        .load()
        .await;

    Ok(CloudWatchClient::new(&shared_config))
}

pub async fn get_secrets_client_with_assumed_role() -> Result<SecretsManagerClient, Box<dyn std::error::Error>> {
    let creds = assume_role(ROLE_ARN).await?;
    let region_provider = RegionProviderChain::default_provider();

    // Directly convert &str to String without `ok_or_else`
    let access_key = creds.access_key_id().to_string();
    let secret_key = creds.secret_access_key().to_string();
    let session_token = creds.session_token().to_string();

    let shared_config = aws_config::from_env()
        .region(region_provider)
        .credentials_provider(Credentials::new(
            access_key,
            secret_key,
            Some(session_token),
            None,
            "CloudwatchAssumedRole",
        ))
        .load()
        .await;

    Ok(SecretsManagerClient::new(&shared_config))
}

static INIT: OnceCell<()> = OnceCell::new();

pub fn init_tracing() {
    INIT.get_or_init(|| {
        let subscriber = FmtSubscriber::builder()
            .with_env_filter(EnvFilter::from_default_env()) // optionally set RUST_LOG
            .with_test_writer() // required to capture test output
            .finish();

        tracing::subscriber::set_global_default(subscriber)
            .expect("Failed to set global tracing subscriber");
    });
}
