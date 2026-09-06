from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from tools.phase4 import publish_release as publisher


class PublishReleaseTests(unittest.TestCase):
    def test_partial_upload_resumes_without_rebuilding_or_replacing_assets(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            assets = Path(directory)
            prefix = "TarnishedsArsenal_1.2.3"
            files = [assets / f"{prefix}{suffix}" for suffix in (
                "_portable.exe", "_x64_en-US.msi", ".zip", "_SHA256SUMS.txt", "_build-report.json",
            )]
            for path in files[:3]:
                path.write_bytes(path.name.encode())
            (assets / "release-notes-v1.2.3.md").write_text("Release notes", encoding="utf-8")

            def write_provenance() -> None:
                files[-1].write_text(json.dumps({
                    "version": "1.2.3", "commit": "a" * 40, "sourceDirty": False,
                    "artifacts": [{"name": path.name, "bytes": path.stat().st_size,
                                   "sha256": publisher.sha256(path)} for path in files[:2]],
                }), encoding="utf-8")
                files[-2].write_text("".join(f"{publisher.sha256(path)}  {path.name}\n" for path in files[:2]), encoding="utf-8")

            write_provenance()
            remote = None
            uploads: list[str] = []
            fail_upload = True
            corrupt_upload = False

            def gh(*args: str, **_kwargs: object) -> str:
                nonlocal fail_upload, remote
                if args[:2] == ("release", "create"):
                    remote = {"isDraft": True, "assets": []}
                    self.assertIn("--draft", args)
                elif args[:2] == ("release", "upload"):
                    assert remote is not None
                    self.assertNotIn("--clobber", args)
                    for filename in args[5:]:
                        path = Path(filename)
                        uploads.append(path.name)
                        remote["assets"].append({"name": path.name, "size": path.stat().st_size,
                                                 "digest": "sha256:" + ("0" * 64 if corrupt_upload else publisher.sha256(path)),
                                                 "state": "uploaded"})
                        if fail_upload:
                            fail_upload = False
                            raise RuntimeError("interrupted upload")
                elif args[:2] == ("release", "edit"):
                    assert remote is not None
                    remote["isDraft"] = False
                elif args[:2] == ("release", "download"):
                    source = assets / args[6]
                    (Path(args[8]) / source.name).write_bytes(source.read_bytes())
                else:
                    self.fail(f"Unexpected mutation: {args}")
                return ""

            with (patch.object(publisher, "release_info", side_effect=lambda *_: remote),
                  patch.object(publisher, "gh", side_effect=gh)):
                with self.assertRaisesRegex(RuntimeError, "interrupted upload"):
                    publisher.publish("owner/repo", "v1.2.3", "a" * 40, assets)
                assert remote is not None
                self.assertTrue(remote["isDraft"])
                self.assertEqual(len(uploads), 1)
                publisher.publish("owner/repo", "v1.2.3", "a" * 40, assets)
                self.assertFalse(remote["isDraft"])
                self.assertCountEqual(uploads, [path.name for path in files])
                remote["assets"][0]["digest"] = None
                publisher.publish("owner/repo", "v1.2.3", "a" * 40, assets)
                self.assertEqual(len(uploads), 5)

                # A rebuilt package at the same commit must fail before uploading anything.
                files[1].write_bytes(b"different installer from a rebuild")
                write_provenance()
                with self.assertRaisesRegex(RuntimeError, "Remote asset differs"):
                    publisher.publish("owner/repo", "v1.2.3", "a" * 40, assets)
                self.assertEqual(len(uploads), 5)

                remote = {"isDraft": True, "assets": []}
                corrupt_upload = True
                with self.assertRaisesRegex(RuntimeError, "Remote asset differs"):
                    publisher.publish("owner/repo", "v1.2.3", "a" * 40, assets)
                self.assertTrue(remote["isDraft"])

    def test_release_lookup_uses_the_cli_draft_aware_lookup(self) -> None:
        with patch.object(publisher, "gh", return_value='{"isDraft": true, "assets": []}') as gh:
            release = publisher.release_info("owner/repo", "v1.2.3")
            assert release is not None
            self.assertTrue(release["isDraft"])
            gh.assert_called_once_with("release", "view", "v1.2.3", "--repo", "owner/repo",
                                       "--json", "isDraft,assets", missing_ok=True)

    def test_publication_job_downloads_the_original_run_artifact(self) -> None:
        workflow = (Path(__file__).resolve().parents[2] / ".github/workflows/release-package.yml").read_text(encoding="utf-8")
        publish_job = workflow.split("\n  publish:\n", 1)[1]
        self.assertIn("needs: package", publish_job)
        self.assertIn("gh run download $env:GITHUB_RUN_ID", publish_job)
        self.assertNotIn("package_release.py", publish_job)
        self.assertNotIn("overwrite_files", publish_job)
        self.assertIn("publish_release.py", publish_job)


if __name__ == "__main__":
    unittest.main()
