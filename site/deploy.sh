#!/usr/bin/env bash
# Build and deploy loadr.io: marketing site + docs → S3 → CloudFront invalidation.
# Usage: AWS_PROFILE=personal ./site/deploy.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST="$ROOT/site/dist"
BUCKET="loadr.io"
DISTRIBUTION_ID="${DISTRIBUTION_ID:-E1O86EYW71WS3}"
AWS_PROFILE="${AWS_PROFILE:-personal}"

echo "==> generating plugin pages"
python3 "$ROOT/site/build-plugins.py"

echo "==> building CSS"
(cd "$ROOT/site" && npx @tailwindcss/cli -i src/input.css -o assets/site.css --minify)

echo "==> building docs"
mdbook build "$ROOT/docs"

echo "==> assembling dist"
rm -rf "$DIST"
mkdir -p "$DIST/assets" "$DIST/docs"
cp "$ROOT/site/index.html" "$ROOT/site/404.html" "$DIST/"
mkdir -p "$DIST/demos"
cp "$ROOT/site/demos.html" "$DIST/demos/index.html"
mkdir -p "$DIST/download"
cp "$ROOT/site/downloads.html" "$DIST/download/index.html"
mkdir -p "$DIST/plugins"
cp -r "$ROOT/site/plugins/." "$DIST/plugins/"
cp "$ROOT/site/assets/site.css" "$ROOT/site/assets/site.js" "$ROOT/site/assets/consent.js" "$ROOT/site/assets/favicon.svg" "$DIST/assets/"
cp -r "$ROOT/docs/book/." "$DIST/docs/"

# Examples: browsable raw files + a download bundle + a generated index page.
echo "==> bundling examples"
mkdir -p "$DIST/examples"
cp -r "$ROOT/examples/." "$DIST/examples/"
tar czf "$DIST/examples.tar.gz" -C "$ROOT" examples
python3 "$ROOT/site/build-examples-index.py" "$ROOT/examples" "$DIST/examples/index.html"

if [ -d "$ROOT/site/videos/out" ]; then
  mkdir -p "$DIST/videos"
  cp "$ROOT/site/videos/out/"*.mp4 "$ROOT/site/videos/out/"*.jpg "$DIST/videos/" 2>/dev/null || true
fi

# Inject the shared nav partial into every page carrying the marker. Single
# source of truth (site/partials/nav.html) — fails loudly if no marker is hit.
echo "==> injecting shared nav"
python3 "$ROOT/site/build-nav.py" "$ROOT/site/partials/nav.html" "$DIST"

# Cache-bust CSS/JS: site.css and site.js carry a 24h cache and stable names,
# so browsers keep serving stale copies after a deploy. Append a content hash
# to their references in the HTML — a new hash = a new URL = guaranteed refetch.
echo "==> cache-busting assets"
CSS_HASH=$(sha256sum "$DIST/assets/site.css" | cut -c1-10)
JS_HASH=$(sha256sum "$DIST/assets/site.js" | cut -c1-10)
CONSENT_HASH=$(sha256sum "$DIST/assets/consent.js" | cut -c1-10)
find "$DIST" -name "*.html" -exec sed -i \
  -e "s#\(assets/site\.css\)\(?v=[0-9a-f]*\)\?#\1?v=${CSS_HASH}#g" \
  -e "s#\(assets/site\.js\)\(?v=[0-9a-f]*\)\?#\1?v=${JS_HASH}#g" \
  -e "s#\(assets/consent\.js\)\(?v=[0-9a-f]*\)\?#\1?v=${CONSENT_HASH}#g" {} +
echo "    css?v=${CSS_HASH}  js?v=${JS_HASH}  consent?v=${CONSENT_HASH}"

echo "==> syncing to s3://$BUCKET"
# Long-lived cache for static assets…
aws-vault exec "$AWS_PROFILE" -- aws s3 sync "$DIST" "s3://$BUCKET" \
  --delete \
  --exclude "*.html" \
  --cache-control "public, max-age=86400" \
  --quiet
# …short cache for HTML so deploys propagate fast.
aws-vault exec "$AWS_PROFILE" -- aws s3 sync "$DIST" "s3://$BUCKET" \
  --exclude "*" --include "*.html" \
  --cache-control "public, max-age=300, must-revalidate" \
  --content-type "text/html; charset=utf-8" \
  --quiet

echo "==> invalidating CloudFront ($DISTRIBUTION_ID)"
aws-vault exec "$AWS_PROFILE" -- aws cloudfront create-invalidation \
  --distribution-id "$DISTRIBUTION_ID" \
  --paths "/*" \
  --query "Invalidation.Id" --output text

echo "==> done: https://loadr.io"
