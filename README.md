# Typr

Typr is a local-first dictation app for macOS and Windows.

The goal is not to build a SaaS clone of Superwhisper or Wispr Flow. The goal is to build a private, fast, developer-friendly voice utility that we can run for ourselves, point at local models, and optionally share with friends who want to run it on their own machines.

## Product Direction

Typr should feel like:

- a local voice utility for builders
- a dictation and rewrite tool that works in any app
- a flexible router between local and remote models

Typr should not try to be:

- a billing product
- a multi-tenant SaaS
- an enterprise admin platform
- a compliance dashboard

## What We Are Building

The intended product has three layers:

1. `Capture`
   - Global hotkey
   - Toggle and push-to-talk recording
   - Microphone selection
   - App/selection/clipboard context capture
   - Reliable paste or replace in any app

2. `Transcribe`
   - Local ASR by default
   - Optional cloud ASR
   - Language selection and auto-detect
   - Streaming later, reliable batch mode first

3. `Transform`
   - Raw dictation mode
   - Polished rewrite mode
   - Command mode for selected text
   - App-aware and developer-aware modes

## Why This Direction

From current public positioning as of April 26, 2026:

- Superwhisper is strongest on local/cloud model flexibility, custom modes, file transcription, and privacy-first workflow.
- Wispr Flow is strongest on default polish, command-style editing, and developer-oriented formatting/workflow improvements.

Our opportunity is to combine the useful parts of both while staying local-first and intentionally small in scope.

## Core Principles

- Local-first by default
- User-supplied providers instead of hosted lock-in
- Developer-grade formatting safety
- Strong privacy posture
- Simpler than the commercial products

## Backend Strategy

Typr should support both local and remote providers behind stable interfaces.

### ASR providers

- Local Whisper backend
- Better local backend later, if we outgrow current choices
- Optional remote providers for users who want them

### LLM transform providers

- LM Studio via OpenAI-compatible endpoint
- Ollama or other local server later
- OpenAI API key for remote usage

Important: we should not assume a personal ChatGPT or Codex subscription can be reused as a generic third-party app backend. For Typr, we should design around:

- local servers like LM Studio
- normal API-key-based providers

## Phase 1

Phase 1 is the first version worth using daily.

- Reliable recording lifecycle
- Reliable paste/replace behavior
- Local transcription that works well
- Language selection
- Raw mode
- Polished mode
- Secure secret storage in OS keychain
- Provider abstraction for ASR and rewrite backends
- LM Studio-backed rewrite mode
- Snippets and replacements
- History and reprocess flow
- Command mode for selected text

## Phase 2

- App-aware context
- Clipboard and selection-aware transforms
- Translation mode
- File transcription
- Meeting mode
- Custom modes and prompts
- Vocabulary and correction learning
- Better per-app behavior

## Phase 3

- Code-aware dictation mode
- Syntax-safe formatting
- camelCase / snake_case / CLI preservation
- Workspace-aware prompt helpers
- Config import/export for friend-shareable setups

## Explicit Non-Goals

These are intentionally out of scope unless our needs change:

- subscriptions
- cloud accounts
- hosted sync
- enterprise admin
- SSO / SAML
- billing
- usage dashboards

## Current Repo Status

The current codebase is still an early prototype. Before expanding features, we should stabilize the core:

- fix recorder state recovery on all failures
- replace destructive text cleanup
- add a real provider abstraction
- support multiple input sample formats
- secure API keys properly
- improve local model install validation
- add real hotkey rebinding

## Docs

- Product and architecture roadmap: [docs/strategy/2026-04-26-local-first-roadmap.md](/Users/leonshimizu/Desktop/ShimizuTechnology/sandbox/typr/docs/strategy/2026-04-26-local-first-roadmap.md)
- Original prototype design: [docs/superpowers/specs/2026-04-08-typr-dictation-app-design.md](/Users/leonshimizu/Desktop/ShimizuTechnology/sandbox/typr/docs/superpowers/specs/2026-04-08-typr-dictation-app-design.md)
- Original implementation plan: [docs/superpowers/plans/2026-04-08-typr-dictation-app.md](/Users/leonshimizu/Desktop/ShimizuTechnology/sandbox/typr/docs/superpowers/plans/2026-04-08-typr-dictation-app.md)
