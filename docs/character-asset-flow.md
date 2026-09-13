# Character asset flow

## Product decision

An imported GLB should belong to the developer's game first. Importing and
testing an asset must never require an account or publish anything to the
network. A second, explicit action can publish a validated asset to the
community catalog when its author chooses a license and makes it reusable.

This gives us two useful paths:

```text
private game asset                         community asset
GLB + sidecar + compiled pack              same package, uploaded once
inside the game repository                 to the author's catalog
```

The game repository remains the source of truth. The community catalog is a
distribution and discovery layer, not the place where a game's source art is
silently stored.

## Proposed developer workflow

1. Run `studio --path /path/to/my-game`. Studio treats that directory as the
   active project, even when the Luau source lives in a separate repository.
2. In Assets or the character workspace, choose **Import character asset…** and
   select a GLB. The first dialog asks what the file is: **new body**,
   **wearable part**, or **full outfit**. It also shows the detected rig,
   nodes, materials, and triangle counts before writing anything.
3. Studio maps the file to the canonical rig, selects Near/Mid/Far nodes,
   previews it in the real renderer, and reports actionable validation errors.
   A new body must provide a compatible skinned base; a wearable must declare
   its occupied slots and fit profiles; a full outfit is represented as a
   composed preset unless it truly replaces the base body.
4. **Add to this game** writes the source GLB, sidecar, compiled pack, and
   catalog entry under the game repository. It updates the game manifest or
   character catalog atomically and reloads the preview. The developer can
   commit those files normally and the game build can reproduce the pack.
5. An optional **Share with community** action appears only after local
   validation. It requires sign-in, asks for a license and attribution, shows
   exactly what will be public, then uploads an immutable version. The game
   still references a pinned asset ID/version; publishing never changes the
   local game automatically.

## Repository shape

The intended project-owned layout is:

```text
my-game/
  assets/
    characters/
      catalog.json
      <asset-slug>/
        source.glb
        source.morph.json
        runtime.morphpack
        thumbnail.png
  manifest.json
  src/main.luau
```

`source.glb` and its sidecar are editable source. `runtime.morphpack` and the
thumbnail are derived, content-addressed build outputs. Luau and the manifest
refer to stable asset IDs, not absolute filesystem paths. Studio should watch
the source files and regenerate or mark derived files stale when they change.

## Authentication flow

Authentication is needed for community publishing, catalog ownership, and
future private cloud features—not for local GLB import. Studio now uses the
existing web login with a short-lived loopback callback:

1. Studio binds `127.0.0.1` on an ephemeral port and generates a random state.
2. It opens the existing cubacadabra login page in the default browser.
3. Google or email login runs entirely in the browser.
4. The web session redirects to Studio's loopback callback. Studio exchanges
   the one-time code for native tokens, closes the listener, and shows a small
   success page: “You're signed in to cubacadabra Studio. You can close this
   browser window.”
5. The access and refresh tokens stay in Studio memory for this first slice;
   the next security step is OS keychain storage so a later launch can restore
   the session without putting credentials in the game repository.

The callback accepts only the registered app scheme or a localhost callback at
`/auth/callback`; arbitrary redirect URLs are rejected by the backend.

## Community catalog guardrails

- Browse can remain public and read-only.
- Publishing requires an authenticated owner and an explicit license.
- Catalog records should include author, attribution, license, source asset
  hash, runtime pack hash, supported rig/fit profiles, and moderation state.
- Game references should pin immutable versions, so a catalog update cannot
  silently change a shipped game.
- Downloaded packs must keep the existing byte and SHA-256 verification.
- Community assets should be opt-in in the Studio library, visually distinct
  from built-in and project-local assets, and never mixed into a game's source
  directory without an explicit **Add to this game** action.

