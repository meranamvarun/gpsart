-- Version 1 accepted arbitrary bytea values. They cannot be authenticated by the
-- SHA-256-only runtime, so revoke them fail-closed before validating the contract.
DELETE FROM guest_sessions WHERE octet_length(token_hash) <> 32;

ALTER TABLE guest_sessions
    ADD CONSTRAINT guest_sessions_sha256_digest_length
    CHECK (octet_length(token_hash) = 32) NOT VALID;

ALTER TABLE guest_sessions
    VALIDATE CONSTRAINT guest_sessions_sha256_digest_length;
