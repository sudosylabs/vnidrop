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

The production build is written to `out/` and can be hosted by any static web server. The GitHub
Pages workflow publishes that directory after relevant changes reach `master` and after a release
is published. The release deployment runs after the GitHub Release exists so the download page can
fetch `release-manifest.json` and render links for the latest public tag.

The canonical production origin defaults to `https://vnidrop.sudosy.fr`. Set
`NEXT_PUBLIC_SITE_URL` only when building for another origin so Open Graph and Twitter image URLs
resolve to that site.
