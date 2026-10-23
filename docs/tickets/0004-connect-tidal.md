# 0004 — Connect Tidal: log in with your Tidal account instead of developer credentials

| | |
|---|---|
| **Status** | Implemented and verified with the developer's account (2026-10-23). Open: login with a second Tidal account (Q5), manual tests L4–L7 |
| **Phase** | 1 → 2 (needed before anyone but the developer gets exact Tidal matches) |
| **Estimate** | ½-day spike + 2–3 days |
| **Depends on** | 0002 |
| **Followed by** | 0005 (Connect Apple Music), which reuses the shared account design from this ticket |

## Problem

Exact Tidal matching (the right release, plus autoplay) needs TIDAL API access. Today each user has to create an app at developer.tidal.com and paste a **client ID and secret** into Settings. That's fine for the developer, but impossible to ask of normal users:
- Nobody should need a developer portal account to use a music utility.
- A client secret can't ship inside the app (it can be extracted, especially from open source).
- Without credentials, Tidal falls back to a **search**, which is noticeably worse.

## Goal

A new **Accounts** section in Settings, starting with Tidal. The user logs in with their normal Tidal account once, and Eddy finds exact matches with *their* login. No secret in the app, no developer portal.

```
Settings → Accounts
  Tidal   [ Connect Tidal ]   →  Tidal login sheet  →  ✓ Connected as you@example.com   [Disconnect]
```

Not connecting is always allowed: Eddy then works as today (Tidal search fallback).

## Shared account design (built here, reused by 0005)

- **A `StreamingAccount` concept per service:** connect, disconnect, status (`notConnected`, `connected(name/region)`, `needsReconnect`), and a way to get a valid credential for API calls.
- **Credentials only in the Keychain** (`kSecClassGenericPassword`, service `com.respectwood.eddy.<service>`), never in `UserDefaults`, logs or debug dumps.
- **The Rust core never touches the Keychain.** The app hands credentials to the resolver, and Rust hands refreshed ones back through a UniFFI **callback interface** (`AccountStore { load(), save(), clear() }`).
- **Errors:** `ResolveError::AccountExpired { service }` when a login is no longer valid, shown as "Reconnect …" in the menu and Settings. Lookups then fall back to today's behaviour instead of failing.
- **Region:** when connected, the lookup country comes from the account instead of `Locale.current`.
- **UI:** Settings → **Accounts** section; the setup flow offers "Connect <service>" right after the user picks a service, with "Skip". The menu shows "Connect <service> for exact matches…" while not connected.
- **Minimum access:** only catalog reads. No library, playlist or playback permissions.

## How Connect Tidal works (OAuth 2.1 + PKCE)

TIDAL supports the standard **Authorization Code flow with PKCE** for apps acting for a user. It's the same flow TIDAL's own SDK uses ([tidal-sdk Auth.md](https://github.com/tidal-music/tidal-sdk/blob/main/Auth.md)). PKCE exists for apps that can't keep a secret, so Eddy ships only a **public client ID**.

1. Generate a random `code_verifier`, its SHA-256 `code_challenge`, and a `state`.
2. Open `https://login.tidal.com/authorize?response_type=code&client_id=…&redirect_uri=eddy://auth/tidal&code_challenge_method=S256&code_challenge=…&state=…` in the **user's own browser** (see spike results; the system login sheet fell back to Safari).
3. Tidal redirects to `eddy://auth/tidal?code=…&state=…`; the browser hands it to eddy.
4. Exchange the code at `https://auth.tidal.com/v1/oauth2/token` (`grant_type=authorization_code`, `code`, `redirect_uri`, `code_verifier`, `client_id`) for access + refresh tokens.
5. Store them in the Keychain and refresh them automatically (`grant_type=refresh_token`) shortly before expiry or after a 401. Refresh errors like `invalid_grant` mean "needs reconnect".
6. Country from `/v2/users/me`.

The existing client ID/secret fields move to a collapsed **"Developer"** section (still useful for development and the CLI).

## Spike first (½ day): open questions

Answer these before building and write the results into this ticket. Several aren't documented publicly.

- [ ] Can a **custom-scheme redirect URI** (`eddy://auth/tidal`) be registered, or must it be `http(s)`? Fallback: a loopback redirect (`http://127.0.0.1:<port>/callback`) served briefly by eddy.
- [ ] Does the token exchange work **without a client secret** (PKCE + `client_id` only)? The SDK docs call the secret optional; it's required only for client credentials.
- [ ] Which **scopes** do catalog reads (`/v2/tracks`, `/v2/albums`, `/v2/searchResults`) need with a user token? Request the minimum.
- [ ] Do catalog endpoints accept a **user token** like a client-credentials token?
- [ ] **Development mode:** can any Tidal account log in, or only allow-listed ones? How far does the per-app usage limit go with the 30-day cache?

If custom schemes or secretless PKCE aren't allowed, re-plan before building.

## Spike results (2026-10-23)

- **Login page:** `login.tidal.com/authorize` is behind DataDome bot protection ("Please enable JS"), so it only works in a real browser. That's fine for `ASWebAuthenticationSession`, but the flow can't be scripted or tested from a terminal.
- **Q2 — no client secret needed: yes.** The token endpoint answers `invalid_grant` (code rejected) to a made-up code with and without the secret, never `invalid_client`, so it accepts public clients. Covered by the live test `tidal_auth::tests::live_token_endpoint_accepts_public_clients`.
- **Q1 — custom-scheme redirect: yes.** The developer portal accepts `eddy://auth/tidal` as a redirect URI.
- **Q3 — scopes: none needed.** New developer apps have *no* allowed scopes, and requesting one that isn't allowed makes the login page fail. Eddy requests no scope; catalog reads work without one. The optional profile (`/v2/users/me`, name and country) needs `user.read` and is skipped when it isn't allowed. The lookup country then comes from the Mac's region.
- **Q4 — catalog endpoints accept a user token: yes.** An uncached lookup (Queen – Bohemian Rhapsody) ran through the user login and resolved to *A Night At The Opera (Deluxe Edition)*, with autoplay verified.
- **Q5 — who can log in in development mode:** only tested with the developer's own account. To check with a second Tidal account before sharing the app.
- **Login UI:** the macOS login sheet (`ASWebAuthenticationSession`) fell back to Safari, because neither Firefox nor Eddy (the default "browser") supports it. The login page now opens in the user's own browser (the one ordinary links go to); Tidal redirects to `eddy://auth/tidal`, which the browser hands to eddy. Each step is logged (never codes or tokens).

## Plan

### Shared (see design above)
- [x] `TidalTokenStore` callback interface (UniFFI) + Keychain implementation in Swift (`KeychainTidalTokenStore`).
- [x] `ResolveError::AccountExpired` → `NotFound { reason: AccountExpired }` with a Tidal search fallback.
- [x] Settings → **Accounts** section (shown when Tidal is chosen); menu hint while not connected or expired.
- [x] `eddy://auth/…` is caught by the login sheet; stray ones are ignored. `state` is checked in Rust.
- [x] Public client ID from `EDDY_TIDAL_CLIENT_ID` (`.env`, untracked) → `Info.plist` `EddyTidalClientID`. Empty → "not available in this build".

### Rust core
- [x] `tidal_auth.rs`: PKCE (`code_verifier` 43–128 chars, `S256`), authorize-URL builder, code exchange, refresh. Unit tests with fixed verifiers.
- [x] `TidalAuth` on `ResolverConfig`: `ClientCredentials { id, secret }` (developer) or `User { client_id }` + `AccountStore`.
- [x] Refresh inside `Tidal::token()`; country from `/v2/users/me` (used as lookup country while connected).

### FFI
- [x] `tidal_login_request(client_id, redirect_uri)` and `tidal_finish_login(request, callback_url)`.
- [x] `Resolver` constructor takes the optional token store (login wins over developer credentials).

### macOS app
- [x] `TidalAccount.swift`: `ASWebAuthenticationSession` (`callbackURLScheme: "eddy"`, non-ephemeral) presented from Settings.
- [x] Accounts UI: Connect / "Connected as …" / Disconnect / Reconnect.
- [x] Move client ID/secret fields under "Developer".

### CLI (optional)
- [ ] `eddy tidal login|logout` with a loopback redirect, tokens in a file under `~/.config/eddy/`.

## Security & privacy

- No secret in the app: PKCE with a public client ID.
- `state` checked against forged callbacks; the code verifier never leaves the Mac.
- Tokens only in the Keychain; grep check that no token appears in logs, `UserDefaults` or debug dumps.
- Minimum scopes: catalog reads only.
- Disconnect deletes local tokens; users can also revoke access in their Tidal account settings.

## Not solved by this ticket

- **Public distribution approval.** TIDAL's developer terms cover non-commercial apps, apps start in development mode, and there's currently no known way to request production review (see PLAN.md risks). Login removes the developer-portal step, but the usage limit stays shared by all users. Contact TIDAL before a public release.

## Acceptance criteria

- [ ] Spike questions answered in this ticket.
- [ ] A user without a developer account clicks Connect Tidal, logs in, and the Usher link opens *Confessions* (exact match) with autoplay.
- [ ] Login survives restarts; tokens refresh automatically (test with a shortened expiry in a debug build).
- [ ] Disconnect → Tidal search fallback; Reconnect works.
- [ ] Revoked access on Tidal's side → "Reconnect Tidal", no error loop.
- [ ] Developer credentials still work under "Developer".
- [ ] No tokens in logs, `UserDefaults` or debug dumps.
- [ ] Manual tests added (section L).
