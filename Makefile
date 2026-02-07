.PHONY: qa qa-lint qa-format qa-typecheck qa-test qa-legacy qa-multi-repo

qa: qa-lint qa-typecheck qa-test qa-legacy
	@echo "All quality checks passed!"

qa-lint:
	uv run poe lint

qa-format:
	uv run poe format

qa-typecheck:
	uv run poe typecheck

qa-test:
	uv run pytest tests/ -v --tb=short

qa-legacy:
	uv run python scripts/find_dead_code.py

qa-multi-repo:
	uv run pytest tests/test_multi_repo_schema.py tests/test_smoke_multi_repo.py -v
