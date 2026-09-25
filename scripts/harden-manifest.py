#!/usr/bin/env python3
"""Apply privacy settings dx's generated manifest does not cover.

Run against the generated AndroidManifest.xml before gradle packages it.
"""
import sys
import re

path = sys.argv[1]
xml = open(path).read()

if "allowBackup" not in xml:
    # Android backs an app's private directory up to the user's cloud account
    # by default. Here that would carry the credential key and the trade
    # database off the device, and nothing in either is worth restoring to a
    # new phone — bots take seconds to re-add.
    xml = re.sub(
        r"<application\b",
        '<application android:allowBackup="false"\n'
        '        android:dataExtractionRules="@xml/data_extraction_rules"\n'
        '        android:fullBackupContent="false"',
        xml,
        count=1,
    )
    open(path, "w").write(xml)
    print("   manifest: backups disabled")
else:
    print("   manifest: already hardened")
