CREATE TABLE IF NOT EXISTS rooms (
  id serial PRIMARY KEY,
  host TEXT NOT NULL,
  token TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS users (
  id UUID PRIMARY KEY,
  username TEXT NOT NULL,
  token TEXT NOT NULL
);