# AWS setup (one-time, manual for milestone 1)

## Created resources (2026-06-10, account 904233124492)

| Resource | Value |
|---|---|
| Live URL | https://labs.brandanmajeske.com |
| S3 bucket | `labs.brandanmajeske.com` (us-west-2, private, OAC-only policy) |
| CloudFront distribution | `E36BRP2R77ZEOB` (d2ddhndx08q33p.cloudfront.net) |
| Origin Access Control | `E2948NBW4SEPMX` |
| ACM certificate | `arn:aws:acm:us-east-1:904233124492:certificate/af5ea92a-c54d-437e-9972-b111925bf351` |
| Route 53 zone | `Z08739182SHS66F7XKAOW` (brandanmajeske.com) — A/AAAA alias for `labs` |
| GitHub OIDC deploy role | **TODO** — needs the GitHub repo to exist (step 4 below) |

Manual deploy (until CI is wired up):

```bash
./build.sh
aws s3 sync dist/ s3://labs.brandanmajeske.com/ --delete --exclude "*.html" \
  --cache-control "public,max-age=86400"
aws s3 cp dist/ s3://labs.brandanmajeske.com/ --recursive \
  --exclude "*" --include "*.wasm" --content-type application/wasm \
  --cache-control "public,max-age=86400" --metadata-directive REPLACE
aws s3 sync dist/ s3://labs.brandanmajeske.com/ --exclude "*" --include "*.html" \
  --cache-control "public,max-age=60,must-revalidate"
aws cloudfront create-invalidation --distribution-id E36BRP2R77ZEOB --paths "/*"
```

Target architecture: private S3 bucket behind CloudFront with Origin Access
Control. IaC (CDK or Terraform) can replace these steps later.

## 1. S3 bucket

- Create a bucket (any region), **block all public access** (default).
- No static-website-hosting mode — CloudFront reads the bucket directly.

## 2. CloudFront distribution

- Origin: the S3 bucket, with **Origin Access Control** (CloudFront creates
  the bucket policy for you when prompted).
- Default root object: `index.html`.
- Viewer protocol policy: redirect HTTP → HTTPS.
- **Compression: on** (gzip + Brotli) — WASM shrinks 30–50%.
- Cache policy: `CachingOptimized` is fine to start; the deploy script sets
  per-object `Cache-Control` headers.
- Later, for SharedArrayBuffer/threads demos: add a response headers policy
  with `Cross-Origin-Opener-Policy: same-origin` and
  `Cross-Origin-Embedder-Policy: require-corp`.

## 3. Domain (optional for milestone 1)

- Request an ACM certificate **in us-east-1** for your domain.
- Add the domain as an alternate name on the distribution.
- Route 53: A/AAAA alias records → the distribution.

## 4. GitHub OIDC deploy role

1. IAM → Identity providers → add `token.actions.githubusercontent.com`
   (audience `sts.amazonaws.com`).
2. Create a role with a trust policy limited to this repo:

   ```json
   {
     "Version": "2012-10-17",
     "Statement": [{
       "Effect": "Allow",
       "Principal": { "Federated": "arn:aws:iam::<ACCOUNT_ID>:oidc-provider/token.actions.githubusercontent.com" },
       "Action": "sts:AssumeRoleWithWebIdentity",
       "Condition": {
         "StringEquals": { "token.actions.githubusercontent.com:aud": "sts.amazonaws.com" },
         "StringLike": { "token.actions.githubusercontent.com:sub": "repo:<GITHUB_USER>/wasm-portfolio:ref:refs/heads/main" }
       }
     }]
   }
   ```

3. Attach a policy allowing `s3:PutObject`, `s3:DeleteObject`, `s3:ListBucket`
   on the bucket, and `cloudfront:CreateInvalidation` on the distribution.

## 5. GitHub repository variables

Settings → Secrets and variables → Actions → Variables:

| Variable             | Value                                  |
|----------------------|----------------------------------------|
| `AWS_ROLE_ARN`       | arn of the role from step 4            |
| `AWS_REGION`         | bucket region                          |
| `S3_BUCKET`          | bucket name                            |
| `CLOUDFRONT_DIST_ID` | distribution id                        |
