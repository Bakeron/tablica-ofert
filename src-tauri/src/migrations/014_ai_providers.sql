-- AI: several providers asked in turn (a list instead of one), one model setting per provider.
INSERT OR REPLACE INTO settings (key, value)
    SELECT 'ai_providers', value FROM settings WHERE key = 'ai_provider' AND value != 'none';
INSERT OR REPLACE INTO settings (key, value)
    SELECT 'ai_model_' || p.value, m.value FROM settings p, settings m
    WHERE p.key = 'ai_provider' AND p.value != 'none' AND m.key = 'ai_model' AND m.value != '';
DELETE FROM settings WHERE key IN ('ai_provider', 'ai_model');
