# Security

PolarityPrompt runs entirely on your computer and sends no telemetry. It goes online only when
you download an extra language under Settings → Languages: the speech model is fetched from this
project's GitHub Releases (or the Vosk website as a fallback) and checked against a SHA-256
checksum before it is installed. The app stores its settings, the practice scripts and
downloaded models in its data folder.

## Reporting a problem

If you find a security problem (for example a script file that makes the app do something it
should not), please report it privately via **Security → Report a vulnerability** on the
[GitHub repository](https://github.com/bosti-hub/polarityprompt/security), not in a public
issue. You will get an answer within a week.

Only the latest release is supported with fixes.
