from __future__ import annotations

import json
import tempfile
import unittest
import zipfile
from pathlib import Path

from agent_tools.research.digest import build_manifest, write_artifacts

DOCX_XML = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>Deep research says the detector should stay local-first.</w:t></w:r></w:p>
  </w:body>
</w:document>
"""

REPO_ROOT = Path(__file__).resolve().parents[3]


class ResearchDigestTests(unittest.TestCase):
    def test_digest_supports_markdown_and_docx(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = Path(tmp_dir)
            input_dir = root / "input"
            input_dir.mkdir()
            markdown_path = input_dir / "notes.md"
            markdown_path.write_text(
                "# Notes\n\nToken pressure should stay separate.\n",
                encoding="utf-8",
            )
            docx_path = input_dir / "report.docx"
            with zipfile.ZipFile(docx_path, "w") as archive:
                archive.writestr(
                    "[Content_Types].xml",
                    (
                        "<?xml version='1.0' encoding='UTF-8'?>"
                        "<Types "
                        "xmlns='http://schemas.openxmlformats.org/package/2006/content-types'>"
                        "</Types>"
                    ),
                )
                archive.writestr("word/document.xml", DOCX_XML)

            manifest = build_manifest(REPO_ROOT, input_dir)
            output_root = root / "artifacts"
            artifact_paths = write_artifacts(manifest, output_root=output_root)
            manifest_json = json.loads((output_root / "manifest.json").read_text())

            self.assertEqual(manifest["supported_source_count"], 2)
            self.assertEqual(len(manifest["sources"]), 2)
            self.assertTrue((output_root / "manifest.json").exists())
            self.assertTrue((output_root / "digest.md").exists())
            self.assertIn(
                "local-first",
                manifest_json["sources"][0]["text"] + manifest_json["sources"][1]["text"],
            )
            self.assertIn("sources", artifact_paths["sources_dir"])
