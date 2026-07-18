## Goal

Add a secure, self-contained `compose.yaml` that builds the local hnbot image, runs `hnbot serve`, and provides persistent Redis deduplication state.

## Architecture

- Run `hnbot` from the repository Dockerfile with `command: ["serve"]`, `.env` configuration, and an internal Redis hostname override.
- Run the official `redis:8-alpine` image without publishing a host port, wait for its health check, and persist AOF data in a named volume.
- Restart both services unless explicitly stopped.
- Add `.dockerignore` so the `.env` secrets and local development artifacts cannot enter the Docker build context.
- Create the `app` user in the final Python image before `COPY --chown=app:app`, then run hnbot as that non-root user.

## Non-Goals

- Do not expose Redis outside the Compose network or add Redis authentication for this local single-host deployment.
- Do not add an hnbot health endpoint or change application runtime code.
- Do not publish or deploy the Compose stack from this environment.

## Assumptions

- Compose users create `.env` from `.env.example` before starting the stack.
- Redis database persistence is desired because it stores the processed-entry deduplication keys.
- Docker Compose runtime validation is unavailable in this WSL because Docker Desktop integration is disabled; use static YAML assertions and document the limitation.

## Plan

- [x] Add `compose.yaml` with hnbot serve, healthy internal Redis, persistent storage, and restart policies; verified all required service values with disposable PyYAML assertions.
- [x] Add `.dockerignore` to exclude `.env`, VCS data, virtual environments, caches, coverage, and build artifacts; verified required patterns with disposable PyYAML assertions.
- [x] Update README Docker instructions with Compose setup, startup, logs, shutdown, and persistence behavior; verified commands reference the parsed `hnbot` service and preserve the Redis volume on normal shutdown.
- [x] Fix the Dockerfile final-stage user contract so `--chown=app:app` resolves and hnbot runs non-root; verified instruction ordering with static assertions and recorded the reusable build gotcha in `MEMORY.md`.
- [x] Run `just all`, YAML assertions, official Compose schema validation, and `git diff --check`; 51 tests passed with 79% coverage, all static container assertions passed, and schema validation returned `ok`.

## Risks

- A missing `.dockerignore` could bake `.env` secrets into the image; explicitly exclude it and verify the rule.
- Redis data loss would cause duplicate notifications after restart; enable AOF and mount `/data` to a named volume.
- Compose could validate differently on another installed version; use current Compose Specification syntax and call out the unavailable Docker runtime check.
- The existing final image references an undefined `app` user; create it before the chowned copy and select it with `USER`.

## Completion Checklist

- [x] `compose.yaml` starts hnbot with `serve` only after Redis is healthy, verified by parsed service definitions and the official Compose schema.
- [x] Redis is private and persistent, verified by no published `ports`, AOF command arguments, and the named `/data` volume.
- [x] `.env` and local build artifacts are excluded, verified by `.dockerignore` assertions.
- [x] README commands reference the actual `hnbot` and `redis` Compose services, verified by review against the parsed config.
- [x] The hnbot image has a resolvable non-root runtime user, verified by Dockerfile instruction ordering and `MEMORY.md` evidence.
- [x] Repository checks passed via `just all` and `git diff --check`; Docker Compose config/build could not run because Docker Desktop WSL integration is unavailable, accepted because official schema and targeted static assertions passed.
