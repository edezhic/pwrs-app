CREATE TABLE todos (
  uuid uuid PRIMARY KEY,
  task TEXT NOT NULL,
  done BOOLEAN NOT NULL DEFAULT FALSE
)