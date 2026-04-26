# Typr Local-First Roadmap

## Purpose

This document captures the intended direction for Typr after reviewing the current prototype and the public feature positioning of Superwhisper and Wispr Flow.

Typr is for local personal use first. If other people want it, they should be able to run it locally for themselves without depending on our infrastructure.

## Product Definition

Typr should be:

- a local-first dictation utility
- an optional local or remote AI rewrite tool
- a better fit for developer workflows than generic dictation software

Typr should not be:

- a hosted subscription product
- a compliance-heavy enterprise platform
- a clone with all of the same business surface area as commercial competitors

## Lessons From Existing Products

### What Superwhisper gets right

- Strong local and cloud model flexibility
- Mode-driven workflows
- Custom vocabulary and replacements
- File transcription and history/reprocess
- Privacy story with real local execution

### What Wispr Flow gets right

- Better default text polish
- Command mode on selected text
- Developer-oriented formatting and workflow marketing
- Shared snippets/dictionary concept

### What we should deliberately skip

- Billing
- Team admin
- Enterprise dashboards
- SSO / SAML
- Hosted proxy complexity unless we later choose to build a service

## Product Shape

Typr should have three product layers.

### 1. Capture

- Global hotkey registration
- Toggle and push-to-talk support
- Microphone selection
- Overlay / status feedback
- Selected text capture
- Clipboard capture
- Active app context capture
- Paste or replace text in any app

### 2. Transcribe

- Local ASR provider by default
- Optional remote ASR provider
- Language selection
- Auto-detect support
- Reliable batch transcription first
- Streaming transcription later if needed

### 3. Transform

- Raw dictation mode
- Polished mode
- Command mode for selected text
- App-aware rewrite mode
- Code-aware rewrite mode
- Custom prompt-based modes

## Recommended Architecture

### Rust core modules

- `capture`
  - microphone input
  - recording lifecycle
  - sample normalization
  - temporary file handling

- `context`
  - selected text
  - clipboard context
  - active app information

- `providers/asr`
  - `AsrProvider` trait
  - local whisper implementation
  - OpenAI implementation
  - other optional cloud implementations later

- `providers/transform`
  - `TransformProvider` trait
  - LM Studio implementation
  - OpenAI API-key implementation
  - other providers later if we actually need them

- `modes`
  - raw
  - polished
  - command
  - email
  - code prompt
  - custom

- `storage`
  - settings
  - snippets
  - replacements
  - vocabulary
  - history
  - provider metadata

- `secrets`
  - keychain / credential manager integration

- `integration`
  - paste / replace
  - hotkey
  - window and overlay

### Frontend responsibilities

- settings and onboarding
- provider configuration
- model management
- mode management
- snippets / replacements / vocabulary UI
- history / reprocess UI

## Provider Strategy

### Local-first recommendation

Default local stack:

- local ASR
- LM Studio for local rewrite mode

This gives:

- privacy
- low recurring cost
- good flexibility
- easy friend-shareable setup

### Remote provider recommendation

Optional remote stack:

- OpenAI API key
- other compatible APIs later only if they solve a real need

Important note:

Typr should not be architected around the assumption that a user can plug in a general ChatGPT or Codex subscription as if it were a normal app API. The safe supported paths are:

- local OpenAI-compatible servers like LM Studio
- standard API-key-backed providers

## Phase 1 Scope

Phase 1 should produce the first version that is actually worth using every day.

### Reliability work

- Fix recorder state recovery on all error paths
- Validate local model installs atomically
- Handle multiple input sample formats
- Improve failure reporting in UI
- Make paste/replace behavior deterministic

### Core feature work

- Local transcription path that works consistently
- Language selection
- Raw mode
- Polished mode
- Secure provider secret storage
- History and reprocess
- Snippets and replacements
- Command mode on selected text
- LM Studio transform backend
- Provider abstraction for ASR and transform layers

### Testing work

- Recording lifecycle tests
- Provider failure-path tests
- Text-format preservation tests
- Local model download validation tests
- Selection/replace integration tests where feasible

## Phase 2 Scope

- App-aware context
- Clipboard-aware transforms
- Translation mode
- File transcription
- Meeting mode
- Vocabulary learning
- Custom modes and prompts
- Per-app behavior rules

## Phase 3 Scope

- Developer syntax-aware dictation
- Better line-break and indentation preservation
- camelCase / snake_case / CLI handling
- Filename / workspace-aware coding helpers
- Export/import configuration bundles

## Phase 1 Outcome

Phase 1 delivered the baseline local-first version:

1. Recorder state now resets safely on failure paths.
2. Text cleanup preserves formatting instead of flattening it.
3. Provider abstractions exist for ASR and rewrite flows.
4. Secrets moved out of plain JSON into OS keychain storage.
5. Hotkey rebinding is supported.
6. History is local, atomic, and reprocessable.
7. Setup/readiness feedback is surfaced in-app.
8. The product direction is now Local Whisper + OpenAI + LM Studio rewrite.

## Working Definition Of Success

Typr is successful if:

- it is cheaper than paying for hosted dictation tools
- it is private enough for normal internal use
- it is fast enough to become a daily habit
- it works especially well for prompting, writing, and coding
- a friend can clone it, configure local models, and use it without our backend
