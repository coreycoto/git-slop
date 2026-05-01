from __future__ import annotations

from git_slop.config import default_config, normalize_config_payload


def test_default_config_writes_schema_2_contract() -> None:
    config = default_config()

    assert config["schema_version"] == 2
    assert config["check"]["fail_on_slop_band"] == "critical"
    assert "fail_on_priority_band" not in config["check"]
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


def test_legacy_priority_band_check_key_maps_to_slop_band() -> None:
    normalized = normalize_config_payload(
        {
            "schema_version": 2,
            "check": {"fail_on_priority_band": "should_refactor"},
        }
    )

    assert normalized["check"]["fail_on_slop_band"] == "high"
    assert "fail_on_priority_band" not in normalized["check"]


def test_new_slop_band_check_key_wins_over_legacy_priority_key() -> None:
    normalized = normalize_config_payload(
        {
            "schema_version": 2,
            "check": {
                "fail_on_priority_band": "must_refactor",
                "fail_on_slop_band": "moderate",
            },
        }
    )

    assert normalized["check"]["fail_on_slop_band"] == "moderate"
    assert "fail_on_priority_band" not in normalized["check"]
