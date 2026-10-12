BEGIN IMMEDIATE;
CREATE TABLE passive_sessions(installation_id TEXT NOT NULL,session_id TEXT NOT NULL,sequence INTEGER NOT NULL,status TEXT NOT NULL,turn_id TEXT,last_event_at_ms INTEGER NOT NULL,PRIMARY KEY(installation_id,session_id));
CREATE TABLE passive_events(event_id TEXT PRIMARY KEY,installation_id TEXT NOT NULL,session_id TEXT NOT NULL,sequence INTEGER NOT NULL,type TEXT NOT NULL,observed_at_ms INTEGER NOT NULL,UNIQUE(installation_id,session_id,sequence));
CREATE TABLE integration_state(id INTEGER PRIMARY KEY CHECK(id=1),connected INTEGER NOT NULL DEFAULT 0);
INSERT INTO integration_state VALUES(1,0);
PRAGMA user_version=2;
COMMIT;
