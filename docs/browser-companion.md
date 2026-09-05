# Chrome/Edge companion

1. Install Lossy in a stable location and complete preferences.
2. Click **Preferences → Set up browser companion**. This registers the current-user native
   messaging host and opens the bundled `browser` folder.
3. Open `chrome://extensions` or `edge://extensions`, enable **Developer mode**, choose
   **Load unpacked**, and select that folder. Expected ID: `bbebeppoampdkokfpfiihnldhhjegoej`.
4. Pin Lossy's toolbar button. On each chosen website, click **Keep drafts on this site** and
   accept the permission request. Test with a synthetic sentence in a plain text box.

This is an unpacked extension, not a store listing. Browser permission requires explicit action;
the installer never edits your browser profile. Managed policies can block this setup. Repeat
registration if the installed executable moves. Firefox/mobile are unsupported.

Profiles, tabs, document instances, routes and editable elements are separate identities.
Returning to the same living editor resumes its card. An observed empty editor finishes the
draft without deleting history. The next message gets a new card. Enter alone is not Send.
Reloading starts a fresh identity to avoid reusing an old editor ID incorrectly.

WhatsApp Web exposes no stable public draft API. Selected internal conversation IDs are used
when available; otherwise each chat selection is isolated. Returning to a contact may create
another card, rather than risk merging contacts. DOM updates can require adapter changes.
Cross-origin frames, closed shadow DOM and inaccessible custom editors are excluded.

Incognito is disabled and rejected again by the worker. Password/read-only fields and fields
labelled as secrets, payment data or codes are skipped. Ordinary prompts can still contain
sensitive data: enable websites selectively. Message history is not scraped.

**Stop on this site** revokes persistence permission immediately. Reload to remove the listener.
Missing drafts: verify the app is enabled/unpaused, companion toggle is on, site access was
granted and the host was registered. The companion captures drafts, not browser clipboard images;
use Snipping Tool or an allowed image editor for bitmap clipboard recovery.

Communication uses local native messaging and a current-user named pipe, not a remote endpoint.
Keep real conversations out of screenshots and issue reports.
