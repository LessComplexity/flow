#!/usr/bin/env python3
"""Package this extension as a .vsix, with no npm dependency.

Why this exists: dropping a folder into ~/.vscode/extensions does NOT work on current
VS Code or Cursor. They keep `extensions.json` in that directory as the authoritative
registry and do not scan for unregistered folders, so a hand-placed directory (or a
symlink to one) is ignored in silence — indistinguishable from a broken extension.

The supported route is `code --install-extension <file>.vsix`, which writes the
registry entry. `vsce` normally builds the .vsix, but it needs npm; a .vsix is just a
ZIP with three things in it, so this builds one directly:

    extension.vsixmanifest   the XML manifest VS Code reads to register it
    [Content_Types].xml      OPC content-type map (required, or install fails)
    extension/...            the extension itself

    python3 editors/vscode/package-vsix.py
    code --install-extension editors/vscode/mapal-lang-0.1.0.vsix
"""

import json
import pathlib
import sys
import zipfile

HERE = pathlib.Path(__file__).parent

# Everything the extension needs at runtime. Deliberately explicit rather than a glob:
# a .vsix should not carry the packaging script or scratch files.
PAYLOAD = [
    "package.json",
    "language-configuration.json",
    "syntaxes/mapal.tmLanguage.json",
    "icons/mapal.svg",
    "README.md",
]

VSIXMANIFEST = """<?xml version="1.0" encoding="utf-8"?>
<PackageManifest Version="2.0.0" xmlns="http://schemas.microsoft.com/developer/vsx-schema/2011" xmlns:d="http://schemas.microsoft.com/developer/vsx-schema-design/2011">
  <Metadata>
    <Identity Language="en-US" Id="{name}" Version="{version}" Publisher="{publisher}" />
    <DisplayName>{display}</DisplayName>
    <Description xml:space="preserve">{description}</Description>
    <Tags>flow,dataflow,compiler</Tags>
    <Categories>Programming Languages</Categories>
    <GalleryFlags>Public</GalleryFlags>
    <Properties>
      <Property Id="Microsoft.VisualStudio.Code.Engine" Value="{engine}" />
      <Property Id="Microsoft.VisualStudio.Code.ExtensionDependencies" Value="" />
      <Property Id="Microsoft.VisualStudio.Code.ExtensionPack" Value="" />
      <Property Id="Microsoft.VisualStudio.Code.ExtensionKind" Value="ui,workspace" />
    </Properties>
  </Metadata>
  <Installation>
    <InstallationTarget Id="Microsoft.VisualStudio.Code" />
  </Installation>
  <Dependencies/>
  <Assets>
    <Asset Type="Microsoft.VisualStudio.Code.Manifest" Path="extension/package.json" Addressable="true" />
    <Asset Type="Microsoft.VisualStudio.Services.Content.Details" Path="extension/README.md" Addressable="true" />
  </Assets>
</PackageManifest>
"""

CONTENT_TYPES = """<?xml version="1.0" encoding="utf-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension=".json" ContentType="application/json" />
  <Default Extension=".vsixmanifest" ContentType="text/xml" />
  <Default Extension=".xml" ContentType="text/xml" />
  <Default Extension=".svg" ContentType="image/svg+xml" />
  <Default Extension=".md" ContentType="text/markdown" />
</Types>
"""


def main():
    manifest = json.loads((HERE / "package.json").read_text())
    name, version = manifest["name"], manifest["version"]
    out = HERE / f"{name}-{version}.vsix"

    missing = [p for p in PAYLOAD if not (HERE / p).exists()]
    if missing:
        print(f"error: payload files missing: {missing}", file=sys.stderr)
        return 1

    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr(
            "extension.vsixmanifest",
            VSIXMANIFEST.format(
                name=name,
                version=version,
                publisher=manifest["publisher"],
                display=manifest["displayName"],
                description=manifest["description"],
                engine=manifest["engines"]["vscode"],
            ),
        )
        z.writestr("[Content_Types].xml", CONTENT_TYPES)
        for rel in PAYLOAD:
            z.write(HERE / rel, f"extension/{rel}")

    # Assert the archive is what an installer expects, rather than trusting it.
    with zipfile.ZipFile(out) as z:
        names = set(z.namelist())
        for required in ("extension.vsixmanifest", "[Content_Types].xml", "extension/package.json"):
            assert required in names, f"{required} missing from the vsix"
        bad = z.testzip()
        assert bad is None, f"corrupt entry: {bad}"

    print(f"wrote {out} ({out.stat().st_size} bytes, {len(PAYLOAD) + 2} entries)")
    print("install with:")
    print(f"  code   --install-extension {out}")
    print(f"  cursor --install-extension {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
