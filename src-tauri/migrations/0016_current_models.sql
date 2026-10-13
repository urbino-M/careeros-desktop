-- Add current models without changing saved defaults or historical tasks.
INSERT OR IGNORE INTO provider_models
(id,provider_id,model_slug,display_name,supports_reasoning,supports_tools,supports_vision,metadata_json) VALUES
('openai:gpt-6.1-sol', 'openai', 'gpt-6.1-sol', 'GPT-6.1-Sol', 1, 1, 1, '{"reasoningLevels":["low","medium","high","xhigh","max","ultra"]}'),
('openai:gpt-6-astra', 'openai', 'gpt-6-astra', 'GPT-6-Astra', 1, 1, 1, '{"reasoningLevels":["low","medium","high","xhigh","max","ultra"]}'),
('openai:gpt-6-sol', 'openai', 'gpt-6-sol', 'GPT-6-Sol', 1, 1, 1, '{"reasoningLevels":["low","medium","high","xhigh","max","ultra"]}'),
('openai:gpt-6-luna', 'openai', 'gpt-6-luna', 'GPT-6-Luna', 1, 1, 1, '{"reasoningLevels":["low","medium","high","xhigh","max"]}');
UPDATE provider_models SET display_name='GPT-5.6 Sol' WHERE id='openai:gpt-5.6-sol';
UPDATE provider_models SET display_name='GPT-5.6 Terra' WHERE id='openai:gpt-5.6-terra';
UPDATE provider_models SET display_name='GPT-5.6 Luna' WHERE id='openai:gpt-5.6-luna';
