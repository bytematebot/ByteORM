-- Shared benchmark schema. Every ORM adapter maps onto exactly these tables,
-- so no ORM gets an advantage from a different physical layout.
--
-- Table names are plain lowercase plurals ("users", "posts") because they must
-- be spelled identically by five code generators, and unquoted identifiers are
-- the only spelling all of them agree on. `SERIAL` primary keys keep id
-- generation on the database side for every ORM.
DROP TABLE IF EXISTS posts CASCADE;
DROP TABLE IF EXISTS users CASCADE;

CREATE TABLE users (
    id         SERIAL PRIMARY KEY,
    email      TEXT NOT NULL UNIQUE,
    username   TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now()
);

CREATE TABLE posts (
    id         SERIAL PRIMARY KEY,
    user_id    INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title      TEXT NOT NULL,
    content    TEXT NOT NULL,
    views      INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now()
);

CREATE INDEX posts_user_id_created_at_idx ON posts (user_id, created_at DESC);
