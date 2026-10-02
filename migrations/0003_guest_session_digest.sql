ALTER TABLE guest_sessions
    ADD CONSTRAINT guest_sessions_sha256_digest_length
    CHECK (octet_length(token_hash) = 32) NOT VALID;

ALTER TABLE guest_sessions
    VALIDATE CONSTRAINT guest_sessions_sha256_digest_length;
