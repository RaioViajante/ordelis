# Security policy

Ordelis (formerly Synapse) is in early development. The desktop app is an interface preview over sample data: it does not run agents or call providers yet, and it is not production-ready. This policy will be updated as that changes.

## Reporting a vulnerability

Please report suspected vulnerabilities **privately**. Do not open a public issue, pull request or discussion that describes one.

Use GitHub's private vulnerability reporting: open this repository's **Security** tab and choose **Report a vulnerability**.

If that option is not available, open an issue titled "Security contact request" that contains **no details** about the problem. A maintainer will respond there, on a best-effort basis, to arrange a private channel.

Please include, as far as you can, the affected component and commit, the steps to reproduce, and the impact. If a report involves a credential or other secret, say where it is but do not paste its value.

## Scope

- This repository: the desktop app, the Python runtime and its safety components, and the build and CI configuration.
- Ordelis builds on code inherited from [MCO](https://github.com/mco-org/mco). A problem that exists only in upstream MCO should also be reported to that project through its own channels. If you are unsure, report it here.
- Problems in third-party agent CLIs or model providers should be reported to those vendors.

## Supported versions

Ordelis has no released versions yet. Security fixes are made on the default branch only. This section will be updated when releases exist.

## Handling

Reports are handled by the maintainer on a best-effort basis. No response or fix time is guaranteed.

Please allow reasonable time to investigate and fix a problem before disclosing it publicly. Reporters who want credit will be credited.
