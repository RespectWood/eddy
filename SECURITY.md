# Security

Eddy is the default web browser on the Macs it runs on, and it can hold the Accessibility permission, so security reports are taken seriously.

## Reporting a vulnerability

Please **don't open a public issue**. Report it privately instead:

- **GitHub:** the repository's **Security** tab → **Report a vulnerability** (private vulnerability reporting).

Include what you found, the Eddy version (menu → About Eddy), your macOS version, and steps or a link that shows the problem. You'll get an answer within a week. Fixes are released as soon as possible, and you're credited in the release notes unless you'd rather not be.

## Supported versions

Only the latest release gets security fixes.

## What's in scope

For example:

- a web page or another app that makes Eddy open something it shouldn't, or run a command;
- getting at Eddy's Tidal login (Keychain) or other stored data;
- getting code into Eddy, or using its Accessibility or Automation permissions;
- Eddy sending data anywhere other than described in [README → Privacy](README.md#privacy).

Out of scope: bugs in Spotify, TIDAL, Apple Music or macOS themselves, and attacks that already need full control of the user's account.

## If Tidal logins leak

Eddy keeps Tidal logins only on the user's Mac. If a vulnerability ever exposed them, the maintainer must notify TIDAL within 24 hours at dataprotectionofficer@tidal.com (TIDAL Developer Terms, Annex 1, 3.11), and affected users are told to disconnect Tidal in Eddy and remove Eddy's access in their TIDAL account.

## How Eddy is hardened

See [docs/tickets/0010-security-hardening.md](docs/tickets/0010-security-hardening.md): hardened runtime on every build, secrets only in the Keychain, HTTPS only, strict link parsing, only registered link types accepted, and checks on every value from an API before it's used in a link.
