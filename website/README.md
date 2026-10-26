# Flik product website

A dependency-free static site in `dist/`, designed for `https://flik.spedo.dev`.
The indigo / steel engineering identity with amber calls to action, SVG mark, wordmark and social image live in `dist/assets/`.
The landing page identifies Flik as a terminal log analyzer immediately and shows the actual interface. Format support, six investigation features, six executable workflow examples, a grouped keyboard reference and Ctrl+K command reference are included. Examples distinguish shell commands from in-app prompts. The jq comparison is scoped to interactive inspection, not JSON transformation or scripting. Synthetic JSON, Kubernetes/CRI and Nginx samples are downloadable from `dist/assets/samples/`.
The website, metadata, accessibility labels, videos and captions are English-only. Fonts and videos are self-hosted; the page has no analytics, third-party video player or external font request.

## Preview and checks

```sh
python3 -m http.server 4173 --bind 127.0.0.1 --directory website/dist
python3 website/scripts/check_site.py
```

The primary videos are active recordings of Flik using only synthetic data in `demo-data/`: Kubernetes JSON beautification and stacked filters with x undo; smart find with include/exclude rules; and error counts with interval changes. Burned-in key prompts and step progression identify the actions as they happen.
They have native playback controls, PNG posters, English WebVTT captions, and textual scenario descriptions. The hero walkthrough loops muted while visible, with pause and fullscreen controls; reduced-motion preferences disable automatic playback. Gallery videos use native playback controls. The visible titles and captions in the recordings are also English.
The root `scripts/generate_site_demos.py` regenerates the English recordings. Its Python dependencies are Pillow, pyte and imageio-ffmpeg; these are not required to serve the website or run Flik.

## Connect the chosen GitHub repository

Edit `dist/site-config.json`:

```json
{"repository":"torxx666/flik","releaseTag":null}
```

The selected public repository is `torxx666/flik`; the configuration already points to it. No binary is advertised until a corresponding stable release is published. `releaseTag: null` requests GitHub's latest published stable release; set a stable tag such as `v0.1.0` to pin it. The page only enables a platform download if the returned release contains the correctly named matching asset. Drafts and prereleases are excluded. If GitHub is unavailable, visitors can still use the repository link. There is no GitHub token in the browser.

Release archives contain `flik` and `LICENSE` at their root. See `docs/RELEASING.md` in the main project for the CI workflow and installer contract. No ready-to-download release is advertised before it exists.

## Hosting

Upload the contents of `dist/` to any static host (including Cloudflare Pages or GitHub Pages). Keep the project source, `.git`, local logs and deployment credentials outside the public document root. `dist/` is the complete deployable site; no application build or backend is needed.

A private Sites preview is also prepared using the local `.openai/hosting.json`. Keep that local deployment identity out of the public source export. Custom-domain DNS values must come from the chosen host; do not guess a CNAME. The canonical URL, social metadata and sitemap already target `https://flik.spedo.dev`.

The website supports 320 px through desktop layouts, reduced motion, visible keyboard focus, keyboard tab navigation for demos, and copy-command fallback. Browser QA verified the three MP4s play, English-only copy, platform selection, and the unreleased-download state. Frequency reports use UTC; S statistics use an explicitly stated 5,000-line sample.
