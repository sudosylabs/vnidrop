# VniDrop website

The product website for VniDrop, built with Next.js and exported as a static site.

## Local development

```bash
# From the repository root:
make run-docs
```

Open [http://localhost:3000](http://localhost:3000).

## Checks

```bash
make check-docs
```

The production build is written to `out/` and can be hosted by any static web server.
The download page fetches `release-manifest.json` from GitHub Releases at build time so file
links match the latest public tag.

Set `NEXT_PUBLIC_SITE_URL` to the canonical production origin when building for deployment so
Open Graph and Twitter image URLs resolve to the public site. Vercel deployment URLs are detected
automatically.
