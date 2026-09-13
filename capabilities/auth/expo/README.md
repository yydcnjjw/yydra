<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# @yydra/auth

Headless authentication lifecycle for Expo H5 and Android. Products supply the
generated API adapter and own their screens and navigation. Subscribe to the
controller snapshot; remount product request/query state when revision changes.
Create each product client with `controller.sessionFetch()` and discard it when
the revision changes. This binds requests to that session, rejects old clients,
and cancels old responses. `authorizedFetch` is the controller's live transport
for its authentication API adapter.

Native credentials and pending login verifiers use SecureStore. Native login
opens the system browser and redeems a verifier-bound handoff over the API.
The web adapter uses HttpOnly cookies and stores no session credential locally.
