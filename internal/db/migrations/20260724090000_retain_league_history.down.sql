DO $$
BEGIN
    RAISE EXCEPTION '20260724090000_retain_league_history is forward-only; restoring retention policies deletes archived league history';
END
$$;
