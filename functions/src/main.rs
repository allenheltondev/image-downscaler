use anyhow::{Context, Result};
use aws_config::BehaviorVersion;
use aws_sdk_s3::{Client as S3Client, primitives::ByteStream};
use image::{ImageFormat, DynamicImage};
use lambda_runtime::{run, service_fn, Error, LambdaEvent};
use serde::{Deserialize, Serialize};
use std::io::Cursor;

const TARGET_WIDTHS: &[u32] = &[480, 960, 1440, 1920];

#[derive(Deserialize)]
struct EventBridgeEvent {
    detail: EventDetail,
}

#[derive(Deserialize)]
struct EventDetail {
    bucket: BucketInfo,
    object: ObjectInfo,
}

#[derive(Deserialize)]
struct BucketInfo {
    name: String,
}

#[derive(Deserialize)]
struct ObjectInfo {
    key: String,
}

#[derive(Serialize)]
struct Response {
    message: String,
}

async fn function_handler(event: LambdaEvent<EventBridgeEvent>) -> Result<Response, Error> {
    let bucket_name = &event.payload.detail.bucket.name;
    let key = decode_key(&event.payload.detail.object.key);

    if bucket_name.is_empty() || key.is_empty() {
        tracing::warn!("Unsupported event payload");
        return Ok(Response {
            message: "Unsupported event payload".to_string(),
        });
    }

    let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let s3_client = S3Client::new(&config);

    if let Err(e) = handle_key(&s3_client, bucket_name, &key).await {
        tracing::error!("Failed to handle key {}: {}", key, e);
        return Err(e.into());
    }

    Ok(Response {
        message: "Successfully processed image".to_string(),
    })
}

async fn handle_key(s3_client: &S3Client, bucket_name: &str, key: &str) -> Result<()> {
    // Download the original image
    let body = match download_object(s3_client, bucket_name, key).await {
        Ok(body) => body,
        Err(e) => {
            tracing::error!("Failed to read source object {}: {}", key, e);
            return Ok(()); // Don't fail the Lambda, just skip this object
        }
    };

    // Try to load as image to validate it's an image file
    let img = match image::load_from_memory(&body) {
        Ok(img) => img,
        Err(e) => {
            tracing::warn!("Skipping non-image object {}: {}", key, e);
            return Ok(());
        }
    };

    let webp_key = to_webp_key(key);
    if webp_key.is_empty() {
        return Ok(());
    }

    // Create main WebP version if it doesn't exist
    if !object_exists(s3_client, bucket_name, &webp_key).await? {
        let webp_body = convert_to_webp(&img, None)?;
        put_webp_object(s3_client, bucket_name, &webp_key, webp_body).await?;
    }

    // Create sized versions
    let max_width = 1440;
    let width_targets: Vec<u32> = TARGET_WIDTHS.iter()
        .filter(|&&width| width <= max_width)
        .copied()
        .collect();

    if width_targets.is_empty() {
        return Ok(());
    }

    // Process all sizes concurrently
    let tasks: Vec<_> = width_targets.into_iter().map(|width| {
        let s3_client = s3_client.clone();
        let bucket_name = bucket_name.to_string();
        let key = key.to_string();
        let img = img.clone();

        tokio::spawn(async move {
            let sized_key = to_sized_webp_key(&key, width);

            if object_exists(&s3_client, &bucket_name, &sized_key).await? {
                return Ok::<(), anyhow::Error>(());
            }

            let resized_body = convert_to_webp(&img, Some(width))?;
            put_webp_object(&s3_client, &bucket_name, &sized_key, resized_body).await?;
            Ok(())
        })
    }).collect();

    // Wait for all tasks to complete
    for task in tasks {
        if let Err(e) = task.await.context("Task join error")? {
            tracing::error!("Failed to process sized image: {}", e);
        }
    }

    Ok(())
}

fn decode_key(key: &str) -> String {
    if key.is_empty() {
        return String::new();
    }

    // Replace + with space, then URL decode
    let key_with_spaces = key.replace('+', " ");
    urlencoding::decode(&key_with_spaces)
        .map(|s| s.into_owned())
        .unwrap_or_else(|_| key.to_string())
}

fn to_webp_key(key: &str) -> String {
    let last_slash = key.rfind('/').unwrap_or(0);
    let last_dot = key.rfind('.');

    match last_dot {
        Some(dot_pos) if dot_pos > last_slash => {
            format!("{}.webp", &key[..dot_pos])
        }
        _ => format!("{}.webp", key),
    }
}

fn to_sized_webp_key(key: &str, width: u32) -> String {
    let last_slash = key.rfind('/').unwrap_or(0);
    let last_dot = key.rfind('.');

    match last_dot {
        Some(dot_pos) if dot_pos > last_slash => {
            format!("{}-{}.webp", &key[..dot_pos], width)
        }
        _ => format!("{}-{}.webp", key, width),
    }
}

async fn object_exists(s3_client: &S3Client, bucket_name: &str, key: &str) -> Result<bool> {
    match s3_client
        .get_object()
        .bucket(bucket_name)
        .key(key)
        .range("bytes=0-0")
        .send()
        .await
    {
        Ok(_) => Ok(true),
        Err(e) => {
            if let Some(service_error) = e.as_service_error() {
                if service_error.is_no_such_key() {
                    return Ok(false);
                }
            }
            Err(e.into())
        }
    }
}

async fn download_object(s3_client: &S3Client, bucket_name: &str, key: &str) -> Result<Vec<u8>> {
    let response = s3_client
        .get_object()
        .bucket(bucket_name)
        .key(key)
        .send()
        .await
        .context("Failed to get object from S3")?;

    let body = response.body.collect().await
        .context("Failed to read object body")?;

    Ok(body.into_bytes().to_vec())
}

fn convert_to_webp(img: &DynamicImage, width: Option<u32>) -> Result<Vec<u8>> {
    let processed_img = match width {
        Some(w) => {
            let height = (img.height() as f64 * w as f64 / img.width() as f64) as u32;
            img.resize(w, height, image::imageops::FilterType::Lanczos3)
        }
        None => img.clone(),
    };

    let mut buffer = Vec::new();
    let mut cursor = Cursor::new(&mut buffer);

    processed_img.write_to(&mut cursor, ImageFormat::WebP)
        .context("Failed to encode image as WebP")?;

    Ok(buffer)
}

async fn put_webp_object(
    s3_client: &S3Client,
    bucket_name: &str,
    key: &str,
    body: Vec<u8>,
) -> Result<()> {
    s3_client
        .put_object()
        .bucket(bucket_name)
        .key(key)
        .body(ByteStream::from(body))
        .content_type("image/webp")
        .cache_control("public, max-age=31536000, immutable")
        .send()
        .await
        .context("Failed to put WebP object to S3")?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt::init();
    run(service_fn(function_handler)).await
}
