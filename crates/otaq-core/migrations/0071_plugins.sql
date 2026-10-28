-- Plugins (the otaq-marketplace README): marketplaces, installed plugins, each project's use of one, and the tracker issues
-- tickets were imported from. Never edit once committed.
--
-- A plugin is installed once for the whole app; `defaults_json` holds its global setting values. A project turns it
-- on in `project_plugins` with its own values. Otaq stores no tokens anywhere: the plugins' CLIs keep their logins.

-- Where plugins come from: the official marketplace (always present, `official` = 1) and any the user added by URL.
-- `index_json` is the last `plugins/index.json` read, with its ETag.
CREATE TABLE plugin_marketplaces (
    id          TEXT    NOT NULL PRIMARY KEY,
    url         TEXT    NOT NULL UNIQUE,
    label       TEXT    NOT NULL,
    official    INTEGER NOT NULL DEFAULT 0,
    index_json  TEXT,
    etag        TEXT,
    fetched_at  BIGINT,
    fetch_error TEXT,
    added_at    BIGINT  NOT NULL
);

-- An installed plugin. `source` is `marketplace` (its files copied under the data folder, `sha256` the release's) or
-- `local` (used in place from `local_path`). `consented` is the fingerprint of what the user agreed to on the
-- consent screen: the CLIs and command lines it runs; a change asks again.
CREATE TABLE plugins (
    id             TEXT    NOT NULL PRIMARY KEY,
    source         TEXT    NOT NULL CHECK (source IN ('marketplace', 'local')),
    marketplace_id TEXT    REFERENCES plugin_marketplaces (id) ON DELETE SET NULL,
    local_path     TEXT,
    version        TEXT    NOT NULL,
    manifest_json  TEXT    NOT NULL,
    sha256         TEXT,
    fingerprint    TEXT    NOT NULL,
    consented      TEXT    NOT NULL,
    defaults_json  TEXT    NOT NULL DEFAULT '{}',
    installed_at   BIGINT  NOT NULL,
    updated_at     BIGINT  NOT NULL
);

-- A project's use of a plugin: one configuration per plugin per project.
CREATE TABLE project_plugins (
    project_id      TEXT    NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    plugin_id       TEXT    NOT NULL REFERENCES plugins (id) ON DELETE CASCADE,
    enabled         INTEGER NOT NULL DEFAULT 1,
    values_json     TEXT    NOT NULL DEFAULT '{}',
    auto_pull       INTEGER NOT NULL DEFAULT 0,
    pull_minutes    INTEGER NOT NULL DEFAULT 15,
    last_pull_at    BIGINT,
    last_pull_error TEXT,
    PRIMARY KEY (project_id, plugin_id)
);

-- A tracker issue a ticket was imported from, with the snapshot Refresh merges against. Deleting the ticket keeps
-- the row with no ticket: the issue is dismissed, and auto-pull never brings it back. No reference to `plugins`, so
-- a removed plugin's tickets keep their links.
CREATE TABLE ticket_links (
    id             TEXT    NOT NULL PRIMARY KEY,
    ticket_id      TEXT    UNIQUE REFERENCES tickets (id) ON DELETE SET NULL,
    project_id     TEXT    NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    plugin_id      TEXT    NOT NULL,
    tracker        TEXT    NOT NULL,
    issue_id       TEXT    NOT NULL,
    issue_key      TEXT    NOT NULL,
    url            TEXT    NOT NULL,
    parent_key     TEXT,
    parent_url     TEXT,
    snapshot_title TEXT    NOT NULL,
    snapshot_body  TEXT    NOT NULL,
    imported_at    BIGINT  NOT NULL,
    fetched_at     BIGINT  NOT NULL,
    UNIQUE (project_id, plugin_id, issue_id)
);
