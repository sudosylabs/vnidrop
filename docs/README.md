# VniDrop website

The product website for VniDrop, built with Next.js and exported as a static site.

## Local development

```bash
# From the repository root:
make run-docs
```

Open [http://localhost:3000](http://localhost:3000).

## Structure

- `app/globals.css` owns design tokens, resets, accessibility helpers, and shared primitives.
- Route-specific styles stay in colocated `page.module.css` files; they are not imported by the root layout.
- Shared modules under `components/` own matching `*.module.css` files.
- The guide and privacy page share `components/document-page.module.css` and
  `components/document-toc.tsx` for document layout, typography, and section navigation.
- `/guide/` covers installation, previews, transfers, saved devices, relay modes,
  troubleshooting, and the current native source builds.
- `/download/` separates the current public release from previews.
- `/og.png` is rendered at build time from the existing brand mark, local fonts,
  and the macOS/iPhone screenshot in `public/shots/hero.png`.

## Checks

```bash
make check-docs
```

The production build is written to `out/` and can be hosted by any static web server. The GitHub
Pages workflow publishes that directory after relevant changes reach `master` and after a release
is published through the normal release workflow. The release deployment runs after the GitHub Release exists so the download page can
fetch `release-manifest.json`. Its per-platform `downloads` index preserves older
versions when a release updates only selected platforms. Each download uses its
original tag and checksum file; legacy manifests remain supported.

Preview releases do not rebuild or deploy the website. The download page fetches
the latest 100 public releases from GitHub's REST API in the browser, then selects
the most recently published available download for each platform from
`preview-VERSION-N` prereleases. Checksums and the preview manifest are required;
Linux requires both DEB and RPM. A Windows-only preview retains the older previews
for other platforms. Links and checksums are pinned to each package's original tag.
No token is shipped to the browser and no visitor data is stored.

The preview section has loading, empty, and request-failure states. A timeout or
GitHub rate limit leaves normal release downloads usable and offers a retry plus
a link to GitHub, including older previews outside the recent-release window.
With JavaScript disabled, the GitHub preview link remains available.

`npm test` covers release-channel separation, complete preview selection, asset
URLs, malformed/incomplete metadata, and API failures. For browser verification,
open `/download/#preview` with an empty list, a complete preview response, and an
API error; check the fallback and retry at desktop and narrow mobile widths.

Keep current source requirements distinct from published binaries: a release
tag can predate the native Windows or Linux frontend even when master uses it.

The canonical production origin defaults to `https://vnidrop.sudosy.fr`. Set
`NEXT_PUBLIC_SITE_URL` only when building for another origin so Open Graph and Twitter image URLs
resolve to that site.
