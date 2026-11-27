# HTML shares

An HTML share holds one HTML file of at most 4 MiB that the reader's browser
runs in a sandboxed frame. flare stores the file unchanged.

## Upload

- `flare share page.html` sends a multipart form. The `share` part names
  `"format": "html"` with an empty `content`, and the `html` part holds the
  file.
- The body limit is `upload_bytes` plus 64 KiB for the form's framing, so a
  file of exactly 4 MiB is admitted. A form with no `html` part keeps the
  `upload_bytes` limit.
- The file must be non-empty UTF-8. Refusals are 422 `flare::shares::invalid`
  for an empty file, 413 `flare::html::too_large` and 422
  `flare::html::not_utf8`.
- JSON with `"format": "html"` is refused with 415 `flare::html::needs_form`.
- A share keeps its kind. A version of another kind is refused with 422
  `flare::versions::kind_changed`.
- A site with no `hosted.content_origin` refuses the upload with 422
  `flare::html::no_content_host`.
- `GET /api/shares/<id>` for an HTML share answers an empty `content`.

## Reading

- `/s/<id>` mints a view token for the signed-in reader and shows
  `<iframe sandbox="allow-scripts" referrerpolicy="no-referrer" allow="">`
  whose source is `<content origin>/v/<token>`. The frame has no
  `allow-same-origin`, so the page runs in an opaque origin of its own.
- The page's Content-Security-Policy gains `frame-src <content origin>`.
- All HTML shares use the one content origin. There are no per-share hosts.

## Content host

- It answers `GET /v/<token>` and `/up`. A request for any other path gets
  404, and a request with any other Host gets 403 `request::host`.
- The request must send `Sec-Fetch-Dest: iframe`, or it gets 403
  `content::not_framed`.
- The token is AES-256-GCM sealed under a key derived from flare's sealing
  key. It holds the share, the version and the viewer, lives 600 seconds and
  can be used more than once. An unknown or forged token gets 404, an expired
  one gets 410.
- Each request checks that the viewer may still read the share. A revoked
  viewer, a missing version and a missing file all get the same 404.
- The reply is `text/html; charset=utf-8` with `nosniff`, `Cache-Control:
  private, no-store`, `Referrer-Policy: no-referrer`, an empty
  `Permissions-Policy`, and a Content-Security-Policy that begins `sandbox
  allow-scripts` with `default-src 'none'`, inline script and style, `data:`
  images and fonts, `connect-src 'none'` and `frame-ancestors <site origin>`.
  It sets no cookie and no CORS header, and reads no cookie.
- The access log names the route `/v/{token}`, never the token.
- The key store is read on every token mint and open, with no cache.
