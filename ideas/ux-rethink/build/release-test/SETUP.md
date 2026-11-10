# Test setup in the VM (written by the lead)

- Build: branch ux-rethink-fixes (ef84b175) in `%USERPROFILE%\otaq-build\rel`, NSIS installer built there and
  installed silently into `%LOCALAPPDATA%\Otaq` (replacing the VM's earlier test install).
- Test folder: `%USERPROFILE%\otaq-reltest`
  - `home\`: the test app's data folder (OTAQ_HOME). Fresh for phase A's first run.
  - `gitconfig`: the git global config the test app uses (GIT_CONFIG_GLOBAL), empty at the start so the
    "git doesn't know your name and email" flow shows. Once saved in the app, it holds the name/email.
  - `shop\`: git repo "pocket-shop" on main: README.md with typos ("libary", "informaton") and a broken link
    (docs/missing.md), src\price.js (total, formatPrice), test.js (`node test.js` prints ok), package.json.
  - `notes\`: a copy of shop (a second project for "Same as…", the switcher, cross-project checks).
  - `shots\`: put screenshots you save here if useful (vmpal_screenshot images are also saved on the Mac).
  - `launch.cmd`: starts the installed app with OTAQ_HOME and GIT_CONFIG_GLOBAL set. Start it on screen with:
    `schtasks /create /tn otaqtest /tr "%USERPROFILE%\otaq-reltest\launch.cmd" /sc once /st 00:00 /it /f` then
    `schtasks /run /tn otaqtest`.
  - The CLI for the test data: `set OTAQ_HOME=%USERPROFILE%\otaq-reltest\home` then `%LOCALAPPDATA%\Otaq\otaq.exe …`
    (check `otaq.exe --help`).
- To quit the app fully: the tray menu's Quit Otaq, or `taskkill /im otaq-app.exe /f` and `taskkill /im otaq.exe /f`
  (the background service) when a phase needs a clean restart.
