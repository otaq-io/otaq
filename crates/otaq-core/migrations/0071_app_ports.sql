-- agents run the app (T4, monorepos): a project's run command becomes a list of named apps, each with its own Run port per ticket.
-- Never edit once committed.
--
-- One row per app port a ticket holds (`web` → 4020, `api` → 4021). Before apps had names every project had one app,
-- which its settings now read as `app`: each ticket's `run_port` moves here as app `app`, and `tickets.run_port` is
-- no longer used (left NULL).
CREATE TABLE app_ports (
    ticket_id TEXT   NOT NULL REFERENCES tickets (id) ON DELETE CASCADE,
    app       TEXT   NOT NULL,
    port      BIGINT NOT NULL,
    PRIMARY KEY (ticket_id, app)
);

INSERT INTO app_ports (ticket_id, app, port)
SELECT id, 'app', run_port FROM tickets WHERE run_port IS NOT NULL;

UPDATE tickets SET run_port = NULL WHERE run_port IS NOT NULL;

-- Which app a Run session ran, so a Run a restart stopped shows "stopped by restart" on its own app (Run output and
-- states live in memory; only this link survives the restart). Sessions from before apps had names have no row: they
-- were app `app`.
CREATE TABLE run_sessions (
    session_id TEXT NOT NULL PRIMARY KEY REFERENCES sessions (id) ON DELETE CASCADE,
    app        TEXT NOT NULL
);
