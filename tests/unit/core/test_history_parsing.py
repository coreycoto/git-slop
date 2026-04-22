from __future__ import annotations

from git_slop import history


def test_parse_status_log_includes_author_metadata_and_rename_edges() -> None:
    raw = (
        "commit\0abc123\01710000000\0Example Dev\0dev@example.com\0"
    ).replace("\x0f", "\0")
    raw += (
        "R100\0src/old.py\0src/new.py\0"
    )

    commits = history._parse_status_log(raw)

    assert commits == [
        {
            "commit": "abc123",
            "timestamp": 10000000,
            "author_name": "Example Dev",
            "author_email": "dev@example.com",
            "author_key": "Example Dev <dev@example.com>",
            "changes": [
                {
                    "status": "R100",
                    "kind": "rename",
                    "old_path": "src/old.py",
                    "new_path": "src/new.py",
                }
            ],
        }
    ]


def test_parse_numstat_log_includes_author_metadata() -> None:
    raw = (
        "commit\0def456\01710000010\0Example Dev\0dev@example.com\0"
    ).replace("\x0f", "\0")
    raw += (
        "5\t3\0src/example.py\0"
    )

    commits = history._parse_numstat_log(raw)

    assert commits == [
        {
            "commit": "def456",
            "timestamp": 10000010,
            "author_name": "Example Dev",
            "author_email": "dev@example.com",
            "author_key": "Example Dev <dev@example.com>",
            "entries": [
                {
                    "added": 5,
                    "deleted": 3,
                    "paths": ["src/example.py"],
                }
            ],
        }
    ]
