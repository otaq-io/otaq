# 059 · A skill bundle is published by the maintainer's signature

- **Date:** 2026-11-14
- **Status:** Accepted
- **Related:** 058, 066

## Summary

One command on the maintainer's Mac signs the current skills and publishes them as a bundle. Otaq accepts only a bundle whose signature matches the key built into the app, as with app updates.

## Decision

- The signing key never leaves the maintainer's Mac.
- A push to a repository, a merge or a CI run never reaches customers by itself.
- An unsigned bundle, or one signed by another key, is refused (decision 066).

## Why

Skills are instructions agents follow with the user's permissions, so whoever can change them controls every
customer's agents. Signing on the Mac keeps that with the key holder even if a GitHub account or token is taken.
Signing in CI on every merge (the key in GitHub's secrets) and following `main` unsigned were turned down. Was S11.
