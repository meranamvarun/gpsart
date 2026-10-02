# Dependency handoffs

Create one file per shared interface using this name: `<story-id>-<interface>-v<version>.md`.

Include producer, consumers, status, compatibility, the exact contract/examples, migration or rollout path, and commands that verify it. Mark it `ready` only when the producer's tests pass. Consumers reference the file in their story evidence before relying on it.
