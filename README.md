<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/ordelis-wordmark-light.png">
    <img src="assets/brand/ordelis-wordmark-dark.png" alt="Ordelis" width="260">
  </picture>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-6B7F73?style=flat-square&labelColor=2E3A33" alt="License: MIT"></a>
  <a href="docs/product/roadmap.md"><img src="https://img.shields.io/badge/status-early%20development-6B7F73?style=flat-square&labelColor=2E3A33" alt="Status: early development"></a>
  <a href="https://github.com/RaioViajante/ordelis"><img src="https://img.shields.io/badge/github-RaioViajante%2Fordelis-6B7F73?style=flat-square&labelColor=2E3A33" alt="GitHub repository: RaioViajante/ordelis"></a>
</p>

<p align="center">
  <strong>Your agents. In order.</strong>
</p>

<p align="center">
  A local-first, open-source desktop workspace<br>
  being built to coordinate AI coding agents.
</p>

<p align="center">
  <sub>Early development. Not ready for production use.</sub>
</p>

---

#### what it is

Ordelis is meant to keep work with coding agents in order: describe a task, plan the work, coordinate the agents, review the evidence, and decide for yourself what gets integrated. It is designed to run on your machine, and it is not a model, a chatbot or an editor. The agents you already use do the coding.

**That complete workflow is not implemented yet.** What exists today:

- A desktop shell that shows sample data. It cannot run agents.
- The command-line runtime inherited from [MCO](https://github.com/mco-org/mco), which can run agents from a terminal. It is not connected to the desktop and is not covered by Ordelis's planned safety model.
- Tested safety foundations, including an execution gate that currently always answers no.

There are no releases, installers or downloads. The desktop has only been built and run on macOS.

#### what's planned

Reviewable plans, agents working in separate Git worktrees, supervised execution, independent review, and integration only when you approve it. None of it is available yet. [Features](docs/product/features.md) lists every planned capability and its current state, and the [roadmap](docs/product/roadmap.md) orders the work without dates.

#### start here

<pre>
00 <a href="docs/product/vision.md">vision</a> ·········· why it exists
01 <a href="docs/product/features.md">features</a> ········ planned capabilities
02 <a href="docs/product/roadmap.md">roadmap</a> ········· the order, no dates

03 <a href="docs/architecture/overview.md">architecture</a> ···· how the parts fit
04 <a href="docs/architecture/agents.md">agents</a> ·········· integrations
05 <a href="docs/architecture/orchestration.md">orchestration</a> ··· plans and delegation
06 <a href="docs/architecture/execution.md">execution</a> ······· controlled execution
07 <a href="docs/architecture/worktrees.md">worktrees</a> ······· one worktree per agent
08 <a href="docs/architecture/context.md">context</a> ········· what agents are shown
09 <a href="docs/architecture/sessions.md">sessions</a> ········ continuity
10 <a href="docs/architecture/review.md">review</a> ·········· independent checks
11 <a href="docs/architecture/cross-platform.md">platforms</a> ······· where it should run

12 <a href="docs/development/README.md">development</a> ····· build and contribute
13 <a href="docs/architecture/security.md">security</a> ········ the security model
14 <a href="SECURITY.md">policy</a> ·········· report a vulnerability
15 <a href="docs/migration/ordelis.md#provenance-and-attribution">origins</a> ········· mco attribution
16 <a href="LICENSE">license</a> ········· mit
</pre>

#### development

```sh
python3 -m unittest discover -s tests -p 'test_*.py'

cd desktop
pnpm install --frozen-lockfile
pnpm lint && pnpm typecheck && pnpm test
cargo test --locked --manifest-path src-tauri/Cargo.toml
```

These assume a checkout and Python 3.10+, Node 22+, pnpm and a stable Rust toolchain; there is no published package. More in [development](docs/development/README.md) and the [desktop commands](desktop/README.md#commands).

#### security

Report vulnerabilities privately: open the repository's **Security** tab and choose **Report a vulnerability**. If you cannot, write to [security@ordelis.app](mailto:security@ordelis.app). Please never open a public issue for one. See [SECURITY.md](SECURITY.md).

#### origins & license

Ordelis was previously named Synapse and began from [MCO](https://github.com/mco-org/mco) `v0.12.1` by the mco maintainers. The inherited runtime keeps its names (the `mco` command, the `runtime` package) and its history. Details are in the [provenance record](docs/migration/ordelis.md#provenance-and-attribution).

Released under the [MIT License](LICENSE), which carries the original copyright notice (Copyright (c) 2026 mco maintainers).
