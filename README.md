# Image Downscaler and WebP Converter

Serverless image processing service that automatically converts uploaded images to optimized WebP format with multiple sizes, served through CloudFront CDN.

## Features

- **Automatic WebP Conversion**: Converts JPG, PNG, GIF, TIFF, and BMP images to WebP format
- **Multiple Sizes**: Generates 480px, 960px, and 1440px wide versions
- **CloudFront CDN**: Global content delivery with intelligent WebP serving
- **Smart Browser Support**: Automatically serves WebP to supporting browsers, falls back to originals
- **S3 Integration**: Event-driven processing on S3 object uploads
- **Custom Domain Support**: Optional custom domain with SSL certificate
- **Rust Performance**: High-performance Lambda function written in Rust

## Architecture

```
S3 Bucket → EventBridge → Lambda (Rust) → S3 (WebP files) → CloudFront → Users
```

1. Upload images to S3 bucket
2. EventBridge triggers Lambda function on new uploads
3. Lambda downloads, processes, and uploads WebP versions
4. CloudFront serves optimized images globally
5. CloudFront Function handles browser compatibility

## Prerequisites

- AWS CLI configured
- SAM CLI installed
- Rust toolchain (1.88+)
- cargo-lambda: `cargo install cargo-lambda`
- Docker (for containerized builds)

## Quick Start

### 1. Deploy the Stack

```bash
# Deploy with SAM
sam build
sam deploy --guided
```

### 2. Configuration Options

During `sam deploy --guided`, you'll be prompted for:

- **S3BucketName**: Leave empty to create new bucket, or specify existing bucket name
- **RootDomainName**: Your domain (e.g., `example.com`) for custom CDN URL (optional)
- **HostedZoneId**: Route53 hosted zone ID for your domain (optional)
- **CustomSubdomain**: Subdomain for accessing images (e.g., `assets`, `cdn`, `images`) (optional)

### 3. Upload Images

Upload images to your S3 bucket:

```bash
aws s3 cp image.jpg s3://your-bucket-name/
```

The Lambda function will automatically:
- Create `<filename>.webp` (original size)
- Create `<filename>-480.webp`, `<filename>-960.webp`, `<filename>-1440.webp` (sized versions)

### 4. Access Images

Use the CloudFront URL from stack outputs with responsive srcset:

```html
<!-- Responsive image with automatic WebP serving -->
<img
  src="https://your-cloudfront-domain/image.jpg"
  srcset="https://your-cloudfront-domain/image-480.jpg 480w,
          https://your-cloudfront-domain/image-960.jpg 960w,
          https://your-cloudfront-domain/image-1440.jpg 1440w"
  sizes="(max-width: 480px) 480px,
         (max-width: 960px) 960px,
         1440px"
  alt="Responsive optimized image">

<!-- Direct WebP with srcset for maximum optimization -->
<img
  src="https://your-cloudfront-domain/image.webp"
  srcset="https://your-cloudfront-domain/image-480.webp 480w,
          https://your-cloudfront-domain/image-960.webp 960w,
          https://your-cloudfront-domain/image-1440.webp 1440w"
  sizes="(max-width: 480px) 480px,
         (max-width: 960px) 960px,
         1440px"
  alt="Responsive WebP image">

<!-- Simple fallback for basic usage -->
<img src="https://your-cloudfront-domain/image.jpg" alt="Auto-optimized">
```

## File Access Patterns

### Public Access (via CloudFront)
- `*.webp` files are publicly accessible
- Original images remain private in S3
- CloudFront serves all content with proper caching headers

### Generated Files
For an uploaded `photo.jpg`, you'll get:
- `photo.webp` (original dimensions)
- `photo-480.webp` (480px wide)
- `photo-960.webp` (960px wide)
- `photo-1440.webp` (1440px wide)

## Custom Domain Setup

To use a custom domain like `cdn.yourdomain.com`:

1. Own a domain with Route53 hosted zone
2. Provide these parameters during deployment:
   - `RootDomainName`: Your domain (e.g., `yourdomain.com`)
   - `HostedZoneId`: Your Route53 hosted zone ID
   - `CustomSubdomain`: Your chosen subdomain (e.g., `cdn`, `assets`, `images`)
3. Stack automatically creates:
   - SSL certificate via ACM
   - Route53 DNS record for `{CustomSubdomain}.{RootDomainName}`
   - CloudFront custom domain configuration

### Example Custom Domain Configurations

- `CustomSubdomain: assets` → `https://assets.yourdomain.com`
- `CustomSubdomain: cdn` → `https://cdn.yourdomain.com`
- `CustomSubdomain: images` → `https://images.yourdomain.com`

## License

MIT License - see LICENSE file for details.
