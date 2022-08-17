use futures::stream::Stream;
use rusoto_core::{ByteStream, Region, RusotoError};
use rusoto_s3::{GetObjectError, GetObjectRequest, PutObjectError, PutObjectRequest, S3Client, S3};
use structopt::StructOpt;
use tokio::io::AsyncReadExt;

#[derive(Clone, StructOpt, Debug, PartialEq)]
pub struct Options {
    /// AWS S3 Storage region for large kafka events
    #[structopt(long, env, default_value = "us-east-1")]
    kafka_region: Region,

    /// AWS S3 Storage bucket for large kafka events
    #[structopt(long, env, default_value = "0x-kafka-large-events")]
    kafka_bucket: String,
}

impl Default for Options {
    fn default() -> Self {
        Options::from_iter(&[""])
    }
}

#[derive(Clone)]
pub struct Storage {
    options: Options,
    client:  S3Client,
}

impl Storage {
    pub fn new(options: Options) -> Self {
        let client = S3Client::new(options.kafka_region.clone());

        // TODO: Test config

        Self { options, client }
    }

    pub async fn upload(
        &self,
        key: String,
        data: Vec<u8>,
    ) -> Result<(), RusotoError<PutObjectError>> {
        let body = ByteStream::from(data);
        let _output = self
            .client
            .put_object(PutObjectRequest {
                bucket: self.options.kafka_bucket.clone(),
                key,
                body: Some(body),
                ..PutObjectRequest::default()
            })
            .await?;
        Ok(())
    }

    pub async fn download(&self, key: String) -> Result<Vec<u8>, RusotoError<GetObjectError>> {
        let output = self
            .client
            .get_object(GetObjectRequest {
                bucket: self.options.kafka_bucket.clone(),
                key,
                ..GetObjectRequest::default()
            })
            .await?;
        // TODO: Appropriate error object
        let body = output
            .body
            .ok_or_else(|| RusotoError::Validation("No body included.".to_string()))?;
        let mut data = Vec::with_capacity(body.size_hint().0);
        let read = body.into_async_read().read_to_end(&mut data).await?;
        assert_eq!(read, data.len());
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use tracing_test::traced_test;

    use super::*;

    #[ignore] // BEWARE: Writes to S3 and doesn't delete test objects
    #[tokio::test]
    #[traced_test]
    async fn test_client() {
        // Create client
        let options = Options::default();
        let client = Storage::new(options);

        // Object
        let key = "test/some/file-data";
        let data = b"Hello, world!".to_vec();

        // Upload
        client.upload(key.to_string(), data.clone()).await.unwrap();

        // Download
        let downloaded = client.download(key.to_string()).await.unwrap();
        assert_eq!(downloaded, data);
    }
}
