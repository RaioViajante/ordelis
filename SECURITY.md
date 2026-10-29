# Security policy

Ordelis is in early development and is not production-ready.

- **The desktop interface is a preview over sample data.** It does not run agents or call providers.
- **One part is real: local project registration.** On macOS the native backend can record the location of folders chosen through the system folder picker in a local registry file. It does not read the contents of those folders. At the time of writing, the interface does not use it yet.
- **The repository also contains the inherited MCO command-line runtime.** Used directly, it can run external agent CLIs. Apart from a development-only status query, it is not connected to the desktop app.

Some code, packages and interface text still use the project's earlier name, Synapse. This policy will be updated as the project changes.

## Reporting a vulnerability

Please report suspected vulnerabilities **privately**, using GitHub's private vulnerability reporting: open this repository's **Security** tab and choose **Report a vulnerability**. This needs a free GitHub account.

Do not open a public issue, pull request or discussion that describes a vulnerability.

If you cannot use private vulnerability reporting, you may send your report privately by email to [security@ordelis.app](mailto:security@ordelis.app). Email is not end-to-end encrypted, so private vulnerability reporting is preferred when you can use it.

Please include, as far as you can, the affected component and commit, the steps to reproduce, and the impact. If a report involves a credential or other secret, say where it is but do not paste its value.

## Scope

- This repository: the desktop app and its native backend, the Python runtime and its safety components, and the build and CI configuration.
- Ordelis builds on code inherited from [MCO](https://github.com/mco-org/mco). A problem that exists only in upstream MCO should also be reported to that project through its own channels. If you are unsure, report it here.
- Problems in third-party agent CLIs or model providers should be reported to those vendors.

## Supported versions

Ordelis has no released versions yet. Security fixes are made on the default branch only. This section will be updated when releases exist.

## Handling

Reports are handled by the maintainer on a best-effort basis. No response or fix time is guaranteed.

Please allow reasonable time to investigate and fix a problem before disclosing it publicly. Reporters who want credit will be credited.
