use aws_sdk_cloudwatch::{Client as CloudWatchClient};
use aws_sdk_cloudwatch::types::{MetricDatum, StandardUnit, Dimension};
use aws_smithy_types::date_time::DateTime;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use aws_config::BehaviorVersion;
use std::env;
use std::sync::Arc;

pub async fn create_cloudwatch_client() -> CloudWatchClient {
    let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    CloudWatchClient::new(&config)
}

/// Emits a CloudWatch metric with a given name, value, and unit.
pub async fn emit_metric(cloud_watch_client: &CloudWatchClient, metric_name: &str, value: f64, unit: StandardUnit) {
    // Fetch environment variable or default to "dev"
    let environment = env::var("ENVIRONMENT").unwrap_or_else(|_| "dev".to_string());
    let namespace = format!("{}/FoxyLambda/Metrics", environment);

    log::info!("Emitting metric {} : {} {}", metric_name, value.to_string(), unit);

    let datum = MetricDatum::builder()
        .metric_name(metric_name)
        .value(value)
        .unit(unit)
        .build();

    if let Err(err) = cloud_watch_client
        .put_metric_data()
        .namespace(namespace)
        .metric_data(datum)
        .send()
        .await
    {
        log::error!(
        "❌ Failed to emit CloudWatch metric '{}': {:?}",
        metric_name,
        err
    );
    }
}

pub async fn emit_fatality(cloud_watch_client: &CloudWatchClient, metric_name: &str) {
    let environment = env::var("ENVIRONMENT").unwrap_or_else(|_| "dev".to_string());
    log::info!("Emitting fatality metric {}", metric_name);

    let _ = cloud_watch_client.put_metric_data()
        .namespace(&format!("{}/FoxyLambda/Metrics", environment))
        .metric_data(
            MetricDatum::builder()
                .metric_name(metric_name)
                .value(1.0)
                .unit(StandardUnit::Count)
                .build()
        )
        .send()
        .await;
}

pub fn emit_broadcast_queue_failure(cloud_watch_client: &CloudWatchClient) {
    let cloud_watch_client = cloud_watch_client.clone(); // Clone to get an owned value

    tokio::spawn(async move {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();

        let smithy_time = DateTime::from_secs(now as i64);

        let datum = MetricDatum::builder()
            .metric_name("BroadcastQueueFailures")
            .timestamp(smithy_time)
            .value(1.0)
            .unit(StandardUnit::Count)
            .dimensions(
                Dimension::builder()
                    .name("Service")
                    .value("FoxyLambda")
                    .build(),
            )
            .build();

        if let Err(err) = cloud_watch_client
            .put_metric_data()
            .namespace("FoxyLambda/TransactionProcessing")
            .metric_data(datum)
            .send()
            .await
        {
            log::error!("Failed to push CloudWatch metric: {:?}", err);
        }
    });
}


pub trait AsF64 {
    fn to_f64(self) -> f64;
}

impl AsF64 for u64 {
    fn to_f64(self) -> f64 {
        self as f64
    }
}

impl AsF64 for u128 {
    fn to_f64(self) -> f64 {
        self as f64
    }
}

pub fn result_to_f64<T, E>(result: &Result<T, E>) -> Option<f64>
where
    T: Copy + AsF64,
{
    result.as_ref().ok().copied().map(|v| v.to_f64())
}

#[macro_export]
macro_rules! track_rpc_call {
    (
        $tracker:expr,
        $label:literal,
        $call:expr
    ) => {{
        use std::time::Instant;
        let __start = Instant::now();
        let __res = $call.await;
        let __elapsed = __start.elapsed().as_millis() as f64;

        $tracker.emit(
            "RpcLatency",
            __elapsed,
            "Milliseconds",
            &[("RPC", $label)],
        ).await;

        if __res.is_err() {
            $tracker.emit(
                "RpcFailures",
                1.0,
                "Count",
                &[("RPC", $label)],
            ).await;
        }

        __res
    }};
}

#[macro_export]
macro_rules! track_ok {
    ($tracker:expr, $expr:expr) => {{
        let __result = $expr.await;
        $tracker.track(&__result, None).await;
        __result
    }};
}

#[derive(Clone, Debug)]
pub struct OperationMetricTracker {
    cloudwatch: Arc<CloudWatchClient>,
    start: Instant,
    environment: String,
    operation: &'static str,  // "Fee", "Gas", etc.
}

impl OperationMetricTracker {
    pub fn new(cloudwatch: CloudWatchClient, operation: &'static str) -> Self {
        let environment = env::var("ENVIRONMENT").unwrap_or_else(|_| "dev".to_string());
        Self {
            cloudwatch: Arc::new(cloudwatch),
            start: Instant::now(),
            environment,
            operation,
        }
    }

    pub async fn build(operation: &'static str) -> Self {
        let cloudwatch = create_cloudwatch_client().await;
        Self {
            cloudwatch: Arc::new(cloudwatch),
            start: Instant::now(),
            environment: env::var("ENVIRONMENT").unwrap_or_else(|_| "dev".to_string()),
            operation,
        }
    }

    pub async fn track<T, E>(&self, result: &Result<T, E>, value: Option<f64>)
    where
        E: std::fmt::Debug + Send + Sync + 'static,
    {
        let status = if result.is_ok() { "Success" } else { "Error" };
        let elapsed = self.start.elapsed().as_millis() as f64;

        self.emit("Latency", elapsed, "Milliseconds", &[("Status", status)])
            .await;

        self.emit("Calls", 1.0, "Count", &[("Status", status)])
            .await;

        if let Some(val) = value {
            self.emit("Value", val, "None", &[("Type", self.operation)])
                .await;
        }
    }

    pub async fn emit_fatal(&self, dependency: &'static str) {
        self.emit("FatalDependencyFailure", 1.0, "Count", &[("Dependency", dependency)]).await;
    }

    pub async fn emit(
        &self,
        metric_name: &str,
        value: f64,
        unit: &str,
        dimensions: &[(&str, &str)],
    ) {
        let namespace = format!("{}/FoxyLambda/Metrics", self.environment);
        let smithy_time = DateTime::from_secs(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_secs() as i64,
        );

        let mut dims = vec![
            Dimension::builder()
                .name("Operation")
                .value(self.operation)
                .build(),
        ];

        dims.extend(dimensions.iter().map(|(k, v)| {
            Dimension::builder()
                .name(*k)
                .value(*v)
                .build()
        }));

        let datum = MetricDatum::builder()
            .metric_name(metric_name)
            .timestamp(smithy_time)
            .value(value)
            .unit(StandardUnit::from(unit))
            .set_dimensions(Some(dims))
            .build();

        if let Err(e) = self
            .cloudwatch
            .put_metric_data()
            .namespace(namespace)
            .metric_data(datum)
            .send()
            .await
        {
            log::error!("Failed to emit {} metric: {:?}", metric_name, e);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use tokio;
    use crate::utilities::test::get_cloudwatch_client_with_assumed_role;

    #[tokio::test]
    async fn test_emit_metric_static_success() -> Result<(), Box<dyn std::error::Error>>{
        // Arrange
        let client = get_cloudwatch_client_with_assumed_role().await
            .expect("cloudwatch client");

        // Act
        emit_metric(&client, "TestStaticMetric", 1.0, StandardUnit::Count).await;

        // Assert: no panic = success for now
        // Optional: you can log something like:
        println!("emit_metric static function executed without crashing");

        Ok(())
    }

    #[tokio::test]
    async fn test_emit_fatality_static_success() -> Result<(), Box<dyn std::error::Error>>{
        let client = get_cloudwatch_client_with_assumed_role().await
            .expect("cloudwatch client");

        emit_fatality(&client, "TestFatalityMetric").await;

        println!("emit_fatality static function executed without crashing");
        Ok(())
    }

    #[tokio::test]
    async fn test_emit_broadcast_queue_failure_spawned() -> Result<(), Box<dyn std::error::Error>>{
        let client = get_cloudwatch_client_with_assumed_role().await
            .expect("cloudwatch client");

        // Call the fire-and-forget method
        emit_broadcast_queue_failure(&client);

        // Sleep briefly to let the spawned task run
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        println!("emit_broadcast_queue_failure executed and returned");
        Ok(())
    }
}
