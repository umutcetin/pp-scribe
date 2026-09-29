# PP Scribe

Meeting transcription for macOS. Records **both sides** of a call, shows live captions while you
talk, then re-transcribes the recording with a more accurate model and saves everything so you can
open it later.

Built on the pp-cursor stack: Tauri v2 + Rust + React 19 / Tailwind 4 / Zustand, cpal audio capture,
Silero VAD, and local STT via `transcribe-rs` (Parakeet + Whisper/Metal). Nothing leaves the machine.

## How it works

```
 mic ───────────────► ChannelPipeline(me)   ─┐     live model (Parakeet V2)
                        ├─ me.wav            ├─► caption worker ──► `caption` events ──► live view
 system audio ──────► ChannelPipeline(them) ─┘      (interim + final)     └─► live.jsonl
 (CoreAudio tap)        ├─ them.wav
                        └─ VAD segmenter

 stop ──► refine: offline VAD per wav ─► ≤28 s speech chunks ─► Whisper Large v3 Turbo ─► refined.json
```

- **Two channels, exact attribution.** "Me" is your microphone, "Them" is whatever the Mac plays
  (captured with a CoreAudio process tap via cpal ≥ 0.17 — no BlackHole needed). A loopback input
  device such as BlackHole can still be used instead (Settings → Other participants).
- **Live captions.** Silero VAD cuts each channel into utterances; an interim transcript is shown
  every 1.5 s while someone is still talking and replaced by the final text when they pause.
- **Accurate transcript.** After the meeting, each recording is re-segmented offline and transcribed
  in chunks with Whisper Large v3 Turbo (previous text of the same speaker is used as a prompt).
  Silence is skipped, which avoids Whisper's hallucinations.
- **Saved meetings.** Everything lives in `~/Documents/PPScribe/<date_time>/`:
  `meta.json`, `me.wav`, `them.wav`, `live.jsonl`, `refined.json`, `transcript.md`.
  Deleting a meeting moves the folder to the Trash.

## Knowledge (local RAG)

A personal knowledge base next to the meetings — **all indexing and search stay on this Mac**.

- **Add:** PDF, Word (.docx), Markdown/text files (Add → Files…, or drop them on the view), notes
  (⌘V anywhere in the view, or Add → Note), and web pages (Add → Link, or paste a URL).
  Meetings are added automatically when they stop (live captions) and refreshed after refine.
- **Search:** hybrid — SQLite FTS5 keywords + [BGE-M3](https://huggingface.co/BAAI/bge-m3) embeddings
  (int8 ONNX, 587 MB, downloaded from the Knowledge view), fused with weighted reciprocal rank fusion.
  Works across Turkish and English. Results jump to the page, heading, or meeting minute.
- **Storage:** `~/Documents/PPScribe/Knowledge/` — `kb.sqlite` plus `files/` (a copy of every
  original). Deleting a source moves its file to the Trash.
- Indexing pauses while a meeting is recording so live captions keep their CPU.

Why BGE-M3 and not multilingual-e5-base: on the smoke set (Turkish questions about English docs and
vice versa) e5 found the right source first in 1 of 4 cases and scored an unrelated query higher
than the real answers; BGE-M3 got 4 of 4 with a clear margin. Check with
`cargo run --example smoke -- kb <db> <files/urls/meeting dirs> -- <queries>`.

Coming next: answers from Claude with citations (only when you switch a question to Claude),
image paste with OCR + image search, and asking during a live meeting.

## Run

```bash
npm install
npm run tauri:dev          # dev (permissions are attributed to your terminal app)
npm run tauri:build -- --debug --bundles app   # PP Scribe.app with its own permission prompts
```

The scripts set `DEVELOPER_DIR=/Library/Developer/CommandLineTools` because Xcode 26.2 is older than
macOS 27 on this machine (see pp-cursor `BUILD-NOTES.md`).

Models are looked up in this app's data dir and in pp-cursor / Handy's model folders, so the existing
Parakeet V2 and Whisper Small downloads are reused. Whisper Large v3 Turbo (874 MB) can be downloaded
from the start screen or Settings.

### Permissions

The first meeting asks for **Microphone** and **System Audio Recording**. If the "Them" meters stay flat,
check System Settings → Privacy & Security → Screen & System Audio Recording.

### Tips

- Wear headphones. With speakers your mic hears the other side too and their words appear twice.
- Parakeet V2 is English only. For Turkish or other languages, set the language and switch the live
  model to Whisper Small; the accurate pass uses Whisper either way.
- System audio is tapped on the **default output device** at meeting start. If your meeting app
  outputs to a different device, make that the system default (or use a loopback device).

## Dev tools

- `preview.html` — browser-only UI harness with a mocked backend (`npx vite`, open `/preview.html`).
- `src-tauri/examples/smoke.rs` — headless pipeline check:
  `cargo run --example smoke -- capture` (plays a `say` sentence, captures mic + system audio,
  prints live captions) and `cargo run --example smoke -- refine file.wav`.

## Layout

```
src-tauri/src/
  audio_toolkit/   capture (mic + system tap), resampler, Silero VAD, utterance segmenter
  stt/             model catalog/downloads, engine slots (live + refine)
  meeting/         live pipeline, refine pass, storage, MeetingManager
  commands.rs      Tauri IPC
src/
  components/      LiveView, MeetingView, TranscriptView, Sidebar, SettingsPanel
  stores/          zustand app store
  hooks/           backend event wiring
```
