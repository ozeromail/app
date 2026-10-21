# 0001. One app repo, one site repo

Accepted, 2026-10-15.

**Context.** The Rust core and both apps change together: a core API change touches the Kotlin and Swift bindings at once. The website has its own stack, hosting and releases.

**Decision.** GitHub organization `ozeromail` with two repos: `app` (core, Android, iOS, push relay, strings) and `site` (landing page).

**Consequences.** A change to the core and both apps lands in one PR. The core has no separate releases.

**Update, 2026-10-21.** Both repos are public, for free CI. The code has no license: it can be read, but all rights are reserved. Secrets stay in Actions secrets, which pull requests from forks never see.
