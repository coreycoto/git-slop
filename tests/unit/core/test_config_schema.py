from __future__ import annotations

from git_slop.config import default_config, normalize_config_payload


def test_default_config_writes_schema_2_contract() -> None:
    config = default_config()

    assert config["schema_version"] == 2
    assert "tokenization" in config
    assert "organization" in config
    assert "verification" in config


def test_schema_1_payload_is_normalized_to_schema_2_with_legacy_aliases() -> None:
    normalized = normalize_config_payload(
        {
            "schema_version": 1,
            "tokenizer": {"name": "cl100k_base"},
            "context_bands": {"warning_max_tokens": 9000},
            "history": {"follow_renames": True},
        }
    )

    assert normalized["schema_version"] == 2
    assert normalized["tokenization"]["context_tokenizer_name"] == "cl100k_base"
    assert normalized["tokenization"]["context_bands"]["warning_max_tokens"] == 9000
    assert normalized["history"]["follow_renames"] is True
    assert normalized["tokenizer"]["name"] == "cl100k_base"
    assert normalized["context_bands"]["warning_max_tokens"] == 9000
