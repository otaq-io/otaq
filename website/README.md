# Otaq website

The landing page at <https://otaq.io>. An [Astro](https://astro.build) project that builds plain HTML; interactive
parts that are more than a few lines (the live demo) are React islands. It is its own project: nothing in `crates/`
depends on it, and `scripts/check.sh` doesn't run it.

## Run it

Node 24 (`.nvmrc`; `engines` says `>=24 <25`), npm. Run everything in this folder.

```sh
npm ci            # install the exact versions in package-lock.json
npm run dev       # dev server at http://localhost:4321, reloads on save
npm run check     # astro check, tsc, ESLint, Prettier check, Vitest, then the build
npm run build     # static site into dist/
npm run preview   # serve dist/ at http://localhost:4321
npm run format    # format every file with Prettier
```

Astro asks to send anonymous usage data when it runs. To turn that off for a shell, set
`ASTRO_TELEMETRY_DISABLED=1` (the deploy workflow does).

## Where things are

- `src/pages/`: one `.astro` file per page (`index.astro` is `/`, the landing page drawn as `Landing3.dc.html`).
- `src/layouts/Base.astro`: every page's `<html>` and `<head>` (title, description, canonical address, link-preview
  tags, favicon) and the **theme script**: an inline `<head>` script that applies the visitor's pick (`localStorage`
  key `otaq-theme`, `dark` or `light`) as `data-theme` on `<html>` before paint. With no pick the page follows the
  system setting.
- `src/components/`: one Astro component per landing section, in page order: `Nav` (with `ThemeToggle`, the
  light/dark switch), `Hero`, `HowItWorks` (with `player.ts` and the pure `flow.ts`), `Flow` (the preset picker, data
  in `presets.ts`), `NeedsYou`, `Review`, `LiveDemo` (the demo's frame, sized by `demoScale.ts`), `BigWork`, `Agents`,
  `Install`, `Faq` (data in `faq.ts`). `Icons.astro` is the SVG sprite, `Icon.astro` draws one, `Mark.astro` is the
  Otaq mark; `theme.ts` is the switch's pure part; `links.ts` holds the repository links. Their tests are in
  `landing.test.ts`.
- `src/demo/`: the live demo (a React island, hydrated `client:idle`), ported from `Demo3.dc.html`: `data.ts` (flows,
  tickets, projects), `state.ts` (the `DemoStore`, tested in `state.test.ts`), `view.ts` (what the components draw),
  the components (`Demo.tsx`, `Board.tsx`, `Sheet.tsx`, `Noyan.tsx`, `FlowEditor.tsx`, `Ic.tsx` with its sprite in
  `icons.ts`) and `demo.css`, scoped under `.oq`.
- `src/styles/tokens.css`: the Otaq Dark and Otaq Light colours as CSS variables; `src/styles/landing.css`: the landing
  page, scoped under `.lp`; `src/styles/fonts.css`: Geist and Geist Mono.
- `public/`: copied to the site as they are: the fonts with their licence (`fonts/LICENSE-geist.txt`, SIL OFL 1.1) and
  the favicon (the app icon), `og.png` (the 1200×630 link-preview image, a screenshot of the hero in Otaq Dark),
  `robots.txt` and `sitemap.xml` (by hand: add a `<url>` there for each new page).
- Tests: `src/**/*.test.ts` and `src/**/*.test.tsx`, run by Vitest in Node. A test that needs a DOM chooses one in its
  own file (`// @vitest-environment …`).

Nothing on the page loads from another site: no Google Fonts, CDN, analytics or cookies.

## Keep it in step with the app

- **Refresh the colours from the app's `tokens.css`** when the app's theme changes: from
  `crates/otaq-desktop/ui/src/theme/generated/tokens.css`, copy the `[data-theme="otaq-dark"]` block into
  `src/styles/tokens.css`'s `:root, :root[data-theme="dark"]` block, and the `[data-theme="otaq-light"]` block into
  both light blocks (`:root:not([data-theme="dark"])` inside `@media (prefers-color-scheme: light)`, and
  `:root[data-theme="light"]`). Copy the values unchanged: the app's tokens meet WCAG AA contrast (decision 0096).
- **Redraw `public/og.png`** when the hero changes: `npm run build && npm run preview`, then a headless Chrome
  screenshot in Otaq Dark. The window is 1300 × 682 scaled down to 1200 × 630, because the hero's floating cards hide
  below 1240 px:

  ```sh
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless --hide-scrollbars \
    --window-size=1300,682 --force-device-scale-factor=0.923077 --force-dark-mode \
    --virtual-time-budget=4000 --screenshot=public/og.png http://localhost:4321/
  ```

  No npm dependency is needed for it.

- **The demo at releases** (decision 0102): when the app's screens have changed, update the demo by hand to match.

## How it deploys

`.github/workflows/website.yml` runs on a push to `main` that changes `website/**` or the workflow file, and by hand
(Actions → Website → Run workflow). It runs `npm ci` and `npm run check`, uploads `dist/` and deploys it to GitHub
Pages. One deploy runs at a time. There is no preview of a change before it is live: check it with `npm run build &&
npm run preview` first.

There is no `CNAME` file: with a GitHub Actions deploy the custom domain is the repository's Pages setting.

## One-time setup (the repository owner)

1. **Pages source.** In `otaq-io/otaq` → Settings → Pages → Build and deployment → Source: **GitHub Actions**.
2. **Verify the domain for the organization.** In the `otaq-io` organization → Settings → Pages → Add a domain:
   `otaq.io`. GitHub shows a TXT record (`_github-pages-challenge-otaq-io.otaq.io`); add it at the DNS provider, then
   press Verify. No other repository can then use the domain.
3. **DNS records** at the DNS provider:

   | Name          | Type  | Value                 |
   | ------------- | ----- | --------------------- |
   | `otaq.io`     | A     | `185.199.108.153`     |
   | `otaq.io`     | A     | `185.199.109.153`     |
   | `otaq.io`     | A     | `185.199.110.153`     |
   | `otaq.io`     | A     | `185.199.111.153`     |
   | `otaq.io`     | AAAA  | `2606:50c0:8000::153` |
   | `otaq.io`     | AAAA  | `2606:50c0:8001::153` |
   | `otaq.io`     | AAAA  | `2606:50c0:8002::153` |
   | `otaq.io`     | AAAA  | `2606:50c0:8003::153` |
   | `www.otaq.io` | CNAME | `otaq-io.github.io`   |

4. **Custom domain.** In the repository → Settings → Pages → Custom domain: `otaq.io`, Save. When the DNS check passes
   and the certificate is ready, tick **Enforce HTTPS**. GitHub then redirects `www.otaq.io` to `otaq.io`.
5. **First deploy.** Push `main` (or run the workflow by hand) and open <https://otaq.io>.
