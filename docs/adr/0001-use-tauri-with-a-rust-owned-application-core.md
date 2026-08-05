# Use Tauri with a Rust-owned application core

The ROM Manager will use Tauri 2 with a Rust application core and a React, strict TypeScript, and Vite presentation layer. This gives the Windows-first application a memory-safe route to WPD through Microsoft's Rust bindings, a narrow privilege seam between the UI and local resources, and an integrated cross-platform packaging toolchain; we accept maintaining two languages, typed IPC, and OS-specific WebView behavior in return.

## Architecture

Rust owns domain rules, workflow orchestration, SQLite persistence, hashing, reconciliation, and Media Target operations. React calls coarse-grained typed commands that return durable operation IDs and receives typed state-change and progress events; persisted snapshots remain authoritative after startup or a missed event. The UI is not granted generic filesystem or SQL access.

The backend begins as one Rust application crate with deep domain, workflow, persistence, transport, and desktop modules. A capability-aware transport interface has filesystem, Windows WPD, and deterministic fake adapters. Windows WPD initially runs in-process through the `windows` crate on a dedicated COM-initialized worker; it moves to a supervised helper only if physical-device testing demonstrates a concrete fault-containment need.

SQLite stores application and operation state, is accessed only by Rust through explicit SQL and checked-in migrations, and serializes writes in short transactions around external I/O. ROM bytes remain outside the database. Tokio coordinates asynchronous work, bounded blocking workers handle filesystem and hashing work, and cancellation is a cooperative request followed by cleanup and an authoritative state refresh.

The repository contains one deployable application and initially avoids package or Cargo workspace decomposition. Cargo manages Rust, pnpm manages TypeScript, and root scripts provide common development and packaging commands. Tauri's CLI and bundler run on native CI hosts, beginning with a Windows NSIS installer and WebView2 Evergreen bootstrapping; final macOS and Linux package formats follow the compatibility decision.

## Considered Options

Avalonia offered a similarly direct, memory-managed WPD route and cohesive C# implementation but required more explicit packaging composition. Electron offered the broadest TypeScript desktop ecosystem at the cost of a bundled Chromium/Node runtime and native integration servicing. Qt provided the shortest native WPD path but added C++ safety and LGPL/commercial licensing costs. Flutter required custom WPD, desktop persistence, and packaging integrations. Tauri was selected because its Rust-native integration and packaging strengths best match the product's Windows-first transfer workload without taking on C++ ownership risk.

## Consequences

The command/event interface and Rust-to-TypeScript data contracts are architecture-critical test surfaces. Transport behavior must be tested primarily through the shared interface and deterministic adapters, with a smaller packaged Windows suite against physical MTP hardware. Remote executable UI content is prohibited, and framework capabilities must remain narrower than the application's domain-oriented commands.
