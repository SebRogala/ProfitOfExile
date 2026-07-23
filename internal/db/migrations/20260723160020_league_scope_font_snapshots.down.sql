DO $$ BEGIN RAISE EXCEPTION '20260723160020_league_scope_font_snapshots is forward-only; restore from a pre-migration backup'; END $$;
