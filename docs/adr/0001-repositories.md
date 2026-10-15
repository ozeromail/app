# 0001. One app repo, one site repo

Accepted, 2026-10-15.

**Context.** The Rust core and both apps change together: a core API change touches the Kotlin and Swift bindings at once. The website has its own stack, hosting and releases.

**Decision.** GitHub organization `ozeromail` with two private repos: `app` (core, Android, iOS, push relay, strings) and `site` (landing page).

**Consequences.** A change to the core and both apps lands in one PR. The core has no separate releases.
