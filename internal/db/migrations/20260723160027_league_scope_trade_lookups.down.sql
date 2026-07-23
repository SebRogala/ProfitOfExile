DO $$ BEGIN RAISE EXCEPTION '20260723160027_league_scope_trade_lookups is forward-only; restore from a pre-migration backup'; END $$;
