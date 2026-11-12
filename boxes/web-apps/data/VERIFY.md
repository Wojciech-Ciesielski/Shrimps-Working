# Weryfikacja katalogu aplikacji internetowych

Sprawdzono: 2026-11-12 15:58 UTC (GitHub Actions, `boxes/web-apps/tools/verify.py`).

Główna lista: 150 pozycji. Odpowiada poprawnie (2xx): 134, serwer odrzuca automat (400/401/403/429/503, pętla przekierowań): 16, błąd (404, 5xx, DNS, TLS, limit czasu): 0. Ikony pobrane: 149.

„zablokowane” znaczy, że serwer istnieje i odpowiada, ale nie wpuszcza skryptu (ochrona przed botami, kod 400 dla nieznanej przeglądarki albo pętla przekierowań bez ciasteczek); adresu nie dało się więc potwierdzić w pełni.

Przekierowanie na stronę logowania albo stronę produktu jest normalne dla aplikacji, które wymagają konta: skrypt nie jest zalogowany.

## Główna lista

| Nazwa | Adres | Wynik | HTTP | Przekierowanie | Manifest PWA | Ikona |
|---|---|---|---|---|---|---|
| WhatsApp | https://web.whatsapp.com/ | zablokowane | 400 |  | nie | favicon 64px, kafelek |
| Telegram | https://web.telegram.org/ | ok | 200 |  | tak | manifest 192px, ostra |
| Messenger | https://www.messenger.com/ | ok | 200 |  | nie | s2 192px, ostra |
| Facebook | https://www.facebook.com/ | zablokowane | 400 |  | nie | apple 180px, ostra |
| Instagram | https://www.instagram.com/ | ok | 200 |  | nie | apple 180px, ostra |
| X | https://x.com/ | ok | 200 |  | tak | manifest 192px, ostra |
| Threads | https://www.threads.com/ | ok | 200 |  | nie | s2 256px, ostra |
| Bluesky | https://bsky.app/ | ok | 200 |  | nie | apple 180px, ostra |
| Mastodon | https://mastodon.social/ | ok | 200 |  | tak | manifest 192px, ostra |
| Reddit | https://www.reddit.com/ | zablokowane | 403 |  | nie | s2 192px, ostra |
| LinkedIn | https://www.linkedin.com/ | ok | 200 |  | nie | icon 512px, ostra |
| Pinterest | https://www.pinterest.com/ | ok | 200 |  | tak | manifest 192px, ostra |
| Discord | https://discord.com/app | ok | 200 |  | nie | s2 256px, ostra |
| Slack | https://app.slack.com/client | ok | 200 |  | nie | favicon 256px, ostra |
| Microsoft Teams | https://teams.microsoft.com/ | ok | 200 | https://teams.microsoft.com/v2/ | tak | manifest 192px, ostra |
| Zoom | https://app.zoom.us/wc | ok | 200 |  | tak | s2 96px, ostra |
| Google Meet | https://meet.google.com/ | ok | 200 | https://workspace.google.com/products/meet/ | nie | ręczna 1024px, ostra |
| Snapchat | https://www.snapchat.com/web | ok | 200 |  | tak | manifest 192px, ostra |
| TikTok | https://www.tiktok.com/ | ok | 200 |  | tak | manifest 192px, ostra |
| Gmail | https://mail.google.com/ | ok | 200 | https://accounts.google.com/v3/signin/identifier?continue=https://mail.google.com/mail/u/0/&emr=1&followup=https://mail.google.com/mail/u/0/&osid=1&passive=1209600&service=mail&flowName=GlifWebSignIn&flowEntry=ServiceLogin&dsh=S436446409:1794499055886369 | nie | ręczna 1024px, ostra |
| Outlook | https://outlook.live.com/mail/ | ok | 200 |  | nie | apple 180px, ostra |
| Proton Mail | https://mail.proton.me/ | ok | 200 |  | tak | manifest 192px, ostra |
| Tuta | https://app.tuta.com/ | ok | 200 |  | nie | apple 512px, ostra |
| Wikipedia | https://pl.wikipedia.org/ | ok | 200 | https://pl.wikipedia.org/wiki/Wikipedia:Strona_g%C5%82%C3%B3wna | nie | apple 160px, ostra |
| ChatGPT | https://chatgpt.com/ | ok | 200 |  | tak | manifest 192px, ostra |
| Claude | https://claude.ai/ | zablokowane | 403 |  | nie | apple 180px, ostra |
| Gemini | https://gemini.google.com/ | ok | 200 |  | tak | manifest 512px, ostra |
| Microsoft Copilot | https://copilot.com/ | ok | 200 |  | nie | s2 48px, kafelek |
| Perplexity | https://www.perplexity.ai/ | ok | 200 |  | nie | apple 180px, ostra |
| Allegro | https://allegro.pl/ | zablokowane | 403 |  | nie | apple 57px, kafelek |
| OLX | https://www.olx.pl/ | ok | 200 |  | nie | apple 192px, ostra |
| Amazon | https://www.amazon.pl/ | ok | 200 |  | nie | s2 48px, kafelek |
| AliExpress | https://www.aliexpress.com/ | zablokowane | 302 | https://login.aliexpress.us/sync_cookie_write.htm?acs_random_token=3a42305f902249f7a9faf64e31c2f90e&xman_goto=https%3A%2F%2Fwww.aliexpress.us%2F%3FgatewayAdapt%3Dglo2usa%26_randl_shipto%3DUS&_ga=GA1.1.398223118030226.1794499058365 | nie | s2 16px, litera |
| PayPal | https://www.paypal.com/ | ok | 200 | https://www.paypal.com/us/home | nie | apple 144px, ostra |
| Google Docs | https://docs.google.com/document/ | ok | 200 | https://accounts.google.com/v3/signin/identifier?continue=https://docs.google.com/document/&followup=https://docs.google.com/document/&ltmpl=docs&osid=1&passive=1209600&service=wise&flowName=GlifWebSignIn&flowEntry=ServiceLogin&dsh=S639346758:1794499056327270 | nie | ręczna 1024px, ostra |
| Google Sheets | https://docs.google.com/spreadsheets/ | ok | 200 | https://accounts.google.com/v3/signin/identifier?continue=https://docs.google.com/spreadsheets/&followup=https://docs.google.com/spreadsheets/&ltmpl=sheets&osid=1&passive=1209600&service=wise&flowName=GlifWebSignIn&flowEntry=ServiceLogin&dsh=S-452358027:1794499056362116 | nie | ręczna 1024px, ostra |
| Google Slides | https://docs.google.com/presentation/ | ok | 200 | https://accounts.google.com/v3/signin/identifier?continue=https://docs.google.com/presentation/&followup=https://docs.google.com/presentation/&ltmpl=slides&osid=1&passive=1209600&service=wise&flowName=GlifWebSignIn&flowEntry=ServiceLogin&dsh=S524009570:1794499056372431 | nie | ręczna 1024px, ostra |
| Google Forms | https://docs.google.com/forms/ | ok | 200 | https://accounts.google.com/v3/signin/identifier?continue=https://docs.google.com/forms/&followup=https://docs.google.com/forms/&ltmpl=forms&osid=1&passive=1209600&service=wise&flowName=GlifWebSignIn&flowEntry=ServiceLogin&dsh=S322151389:1794499056439831 | nie | ręczna 1024px, ostra |
| Google Drive | https://drive.google.com/ | ok | 200 | https://accounts.google.com/v3/signin/identifier?continue=https://drive.google.com/&emr=1&followup=https://drive.google.com/&osid=1&passive=1209600&service=wise&flowName=GlifWebSignIn&flowEntry=ServiceLogin&dsh=S1983413096:1794499056452283 | nie | ręczna 1024px, ostra |
| Google Calendar | https://calendar.google.com/ | ok | 200 | https://accounts.google.com/v3/signin/identifier?continue=https://calendar.google.com/calendar/u/0/r&emr=1&followup=https://calendar.google.com/calendar/u/0/r&osid=1&passive=1209600&service=cl&flowName=GlifWebSignIn&flowEntry=ServiceLogin&dsh=S639346758:1794499056544460 | nie | ręczna 1024px, ostra |
| Google Keep | https://keep.google.com/ | ok | 200 | https://accounts.google.com/v3/signin/identifier?continue=https://keep.google.com/&followup=https://keep.google.com/&ltmpl=keep&passive=1209600&service=memento&flowName=GlifWebSignIn&flowEntry=ServiceLogin&dsh=S1983413096:1794499056592114 | nie | ręczna 1024px, ostra |
| Microsoft 365 | https://m365.cloud.microsoft/ | ok | 200 |  | nie | s2 48px, kafelek |
| Word | https://word.cloud.microsoft/ | ok | 200 | https://word.cloud.microsoft/pl-pl/ | nie | apple 144px, ostra |
| Excel | https://excel.cloud.microsoft/ | ok | 200 | https://excel.cloud.microsoft/pl-pl/ | nie | apple 144px, ostra |
| PowerPoint | https://powerpoint.cloud.microsoft/ | ok | 200 | https://powerpoint.cloud.microsoft/pl-pl/ | nie | apple 144px, ostra |
| OneDrive | https://onedrive.live.com/ | ok | 200 | https://www.microsoft.com/en-us/microsoft-365/onedrive/online-cloud-storage | nie | s2 128px, ostra |
| OneNote | https://onenote.cloud.microsoft/ | ok | 200 |  | nie | apple 144px, ostra |
| Dropbox | https://www.dropbox.com/home | ok | 200 | https://www.dropbox.com/login?cont=%2Fhome | nie | s2 32px, litera |
| Proton Drive | https://drive.proton.me/ | ok | 200 |  | tak | manifest 192px, ostra |
| Notion | https://www.notion.so/ | ok | 200 | https://www.notion.com/ | nie | apple 512px, ostra |
| Todoist | https://app.todoist.com/ | ok | 200 | https://app.todoist.com/auth/login | tak | manifest 192px, ostra |
| Trello | https://trello.com/ | ok | 200 |  | nie | s2 256px, ostra |
| Asana | https://app.asana.com/ | ok | 200 | https://app.asana.com/-/login | nie | apple 72px, kafelek |
| Atlassian (Jira, Confluence) | https://start.atlassian.com/ | ok | 200 |  | nie | ddg 48px, kafelek |
| ClickUp | https://app.clickup.com/ | ok | 200 |  | tak | ddg 32px, litera |
| monday.com | https://monday.com/ | ok | 200 | https://monday.com/lang/pl | nie | apple 180px, ostra |
| Linear | https://linear.app/ | ok | 200 |  | tak | manifest 192px, ostra |
| Miro | https://miro.com/app/ | ok | 200 | https://miro.com/pl/signup/ | nie | s2 128px, ostra |
| Overleaf | https://www.overleaf.com/ | ok | 200 |  | tak | manifest 512px, ostra |
| Grammarly | https://app.grammarly.com/ | ok | 200 | https://www.grammarly.com/ | nie | s2 48px, kafelek |
| LanguageTool | https://languagetool.org/editor | ok | 200 | https://languagetool.org/pl/user-login | tak | manifest 192px, ostra |
| Google Classroom | https://classroom.google.com/ | ok | 200 | https://accounts.google.com/v3/signin/identifier?continue=https://classroom.google.com/&emr=1&followup=https://classroom.google.com/&passive=1209600&service=classroom&flowName=GlifWebSignIn&flowEntry=ServiceLogin&dsh=S436446409:1794499057929651 | nie | s2 32px, litera |
| Duolingo | https://www.duolingo.com/ | ok | 200 |  | tak | manifest 192px, ostra |
| YouTube | https://www.youtube.com/ | ok | 200 |  | tak | manifest 192px, ostra |
| Netflix | https://www.netflix.com/ | ok | 200 |  | nie | apple 64px, kafelek |
| Disney+ | https://www.disneyplus.com/ | ok | 200 |  | tak | manifest 192px, ostra |
| Prime Video | https://www.primevideo.com/ | ok | 200 | https://www.primevideo.com/offers/nonprimehomepage?ref_=dv_web_force_root | nie | apple 152px, ostra |
| HBO Max | https://play.hbomax.com/ | ok | 200 | https://www.hbomax.com/ | nie | apple 180px, ostra |
| Apple TV | https://tv.apple.com/ | ok | 200 |  | tak | manifest 192px, ostra |
| SkyShowtime | https://www.skyshowtime.com/ | ok | 200 | https://www.skyshowtime.com/where-can-i-stream | nie | icon 180px, ostra |
| Player | https://player.pl/ | ok | 200 |  | nie | apple 180px, ostra |
| Polsat Box Go | https://polsatboxgo.pl/ | ok | 200 | https://polsatboxgo.pl/start?pk=eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJhY2Nlc3NHcm91cHMiOlsiYXBwOmFsbCIsImFwcDpicm93c2VyIiwiYXBwOmNocm9tZSIsImJ1aWxkOjMiLCJjcHVzZXI6ZmFsc2UiLCJkZXY6YWxsIiwiZGV2OnBjIiwiZm9yY2VwbDpmYWxzZSIsImxlZ2FjeV91c2VyOmZhbHNlIiwibG9jOmFsbCIsImxvYzp3b3JsZCIsImxvZ2dlZDpmYWxzZSIsIm9zOmFsbCIsIm9zOmxpbnV4IiwicGxheWVyOmFsbCIsInBsYXllcjpodG1sIiwicGx1c3VzZXI6ZmFsc2UiLCJwb3J0YWw6cGJnIiwicHJlc3M6ZmFsc2UiLCJ0ZXN0ZXI6ZmFsc2UiLCJ0ZXN0ZXJfcGVyOmZhbHNlIiwidmVyaWZpZWQ6ZmFsc2UiLCJ2cG46ZmFsc2UiXSwiY29uc3RhbnRzIjpbInJvZG9FeHRlcm5hbENvb2tpZXNFbmFibGVkIl19.GcCI9LEIXduSkGZw8aQ7tdKFHPEy3Y6S_O5SohwObBk | nie | s2 180px, ostra |
| TVP VOD | https://vod.tvp.pl/ | ok | 200 |  | tak | s2 60px, kafelek |
| CDA | https://www.cda.pl/ | ok | 200 |  | tak | apple 192px, ostra |
| Crunchyroll | https://www.crunchyroll.com/ | zablokowane | 403 |  | nie | s2 120px, ostra |
| Twitch | https://www.twitch.tv/ | ok | 200 |  | nie | apple 180px, ostra |
| Vimeo | https://vimeo.com/ | ok | 200 |  | nie | apple 180px, ostra |
| Plex | https://app.plex.tv/ | ok | 200 | https://app.plex.tv/desktop/ | nie | apple 144px, ostra |
| Clipchamp | https://app.clipchamp.com/ | ok | 200 | https://app.clipchamp.com/consumer/ | nie | icon 512px, ostra |
| CapCut | https://www.capcut.com/editor | ok | 200 |  | tak | manifest 128px, ostra |
| Spotify | https://open.spotify.com/ | ok | 200 |  | tak | manifest 128px, ostra |
| YouTube Music | https://music.youtube.com/ | ok | 200 |  | tak | manifest 144px, ostra |
| Apple Music | https://music.apple.com/ | ok | 200 | https://music.apple.com/us/new | tak | manifest 192px, ostra |
| Amazon Music | https://music.amazon.com/ | ok | 200 |  | nie | apple 256px, ostra |
| Tidal | https://listen.tidal.com/ | zablokowane | 403 | https://tidal.com/ | nie | s2 48px, kafelek |
| Deezer | https://www.deezer.com/ | ok | 200 | https://www.deezer.com/pl/ | nie | apple 180px, ostra |
| SoundCloud | https://soundcloud.com/ | ok | 200 |  | tak | manifest 192px, ostra |
| Bandcamp | https://bandcamp.com/ | zablokowane | 403 |  | nie | s2 180px, ostra |
| Qobuz | https://play.qobuz.com/ | ok | 200 |  | tak | apple 180px, ostra |
| Pocket Casts | https://pocketcasts.com/play | ok | 200 |  | tak | manifest 192px, ostra |
| Audible | https://www.audible.com/ | ok | 200 |  | nie | apple 180px, ostra |
| Open FM | https://open.fm/ | ok | 200 |  | tak | manifest 192px, ostra |
| Radio Garden | https://radio.garden/ | ok | 200 |  | tak | apple 180px, ostra |
| Last.fm | https://www.last.fm/ | ok | 200 |  | nie | apple 1024px, ostra |
| Canva | https://www.canva.com/ | zablokowane | 403 |  | nie | s2 193px, ostra |
| Figma | https://www.figma.com/ | ok | 200 |  | tak | manifest 192px, ostra |
| Photopea | https://www.photopea.com/ | ok | 200 |  | tak | manifest 512px, ostra |
| Adobe Express | https://express.adobe.com/ | ok | 200 |  | tak | manifest 192px, ostra |
| Pixlr | https://pixlr.com/ | ok | 200 | https://pixlr.com/pl/ | tak | manifest 192px, ostra |
| Penpot | https://design.penpot.app/ | ok | 200 |  | nie | icon 512px, ostra |
| Excalidraw | https://excalidraw.com/ | ok | 200 |  | tak | manifest 192px, ostra |
| tldraw | https://www.tldraw.com/ | ok | 200 |  | tak | manifest 193px, ostra |
| draw.io | https://app.diagrams.net/ | ok | 200 |  | tak | manifest 192px, ostra |
| Google Zdjęcia | https://photos.google.com/ | ok | 200 | https://www.google.com/photos/about/ | nie | icon 512px, ostra |
| remove.bg | https://www.remove.bg/ | ok | 200 | https://www.remove.bg/pl | tak | manifest 192px, ostra |
| Squoosh | https://squoosh.app/ | ok | 200 |  | tak | manifest 1024px, ostra |
| Coolors | https://coolors.co/ | zablokowane | 403 |  | nie | s2 192px, ostra |
| Tinkercad | https://www.tinkercad.com/ | ok | 200 |  | tak | manifest 192px, ostra |
| Chess.com | https://www.chess.com/ | ok | 200 |  | tak | manifest 192px, ostra |
| Lichess | https://lichess.org/ | ok | 200 |  | tak | manifest 192px, ostra |
| GeForce NOW | https://play.geforcenow.com/ | ok | 200 | https://play.geforcenow.com/mall/ | tak | manifest 192px, ostra |
| Xbox Cloud Gaming | https://play.xbox.com/ | ok | 200 |  | tak | manifest 192px, ostra |
| Boosteroid | https://cloud.boosteroid.com/ | zablokowane | 307 |  | nie | s2 48px, kafelek |
| Steam | https://store.steampowered.com/ | ok | 200 |  | nie | s2 256px, ostra |
| itch.io | https://itch.io/ | ok | 200 |  | tak | manifest 144px, ostra |
| Poki | https://poki.com/ | ok | 200 | https://poki.com/pl#utm_source=redirect-en-pl | tak | manifest 192px, ostra |
| CrazyGames | https://www.crazygames.com/ | ok | 200 |  | tak | manifest 192px, ostra |
| GeoGuessr | https://www.geoguessr.com/ | ok | 200 |  | tak | manifest 192px, ostra |
| Board Game Arena | https://boardgamearena.com/ | ok | 200 | https://pl.boardgamearena.com/ | tak | manifest 192px, ostra |
| Wordle | https://www.nytimes.com/games/wordle/index.html | ok | 200 |  | tak | apple 192px, ostra |
| GitHub | https://github.com/ | ok | 200 |  | tak | manifest 192px, ostra |
| GitLab | https://gitlab.com/ | ok | 200 | https://about.gitlab.com/ | nie | apple 192px, ostra |
| Codeberg | https://codeberg.org/ | ok | 200 |  | tak | manifest 256px, ostra |
| Visual Studio Code | https://vscode.dev/ | ok | 200 |  | nie | brak (litera z aplikacji) |
| CodePen | https://codepen.io/ | zablokowane | 403 |  | nie | s2 180px, ostra |
| CodeSandbox | https://codesandbox.io/ | ok | 200 |  | nie | s2 48px, kafelek |
| StackBlitz | https://stackblitz.com/ | ok | 200 |  | nie | icon 512px, ostra |
| Google Colab | https://colab.research.google.com/ | ok | 200 |  | nie | icon 128px, ostra |
| Stack Overflow | https://stackoverflow.com/ | zablokowane | 403 |  | nie | s2 180px, ostra |
| MDN Web Docs | https://developer.mozilla.org/ | ok | 200 | https://developer.mozilla.org/en-US/ | tak | manifest 192px, ostra |
| regex101 | https://regex101.com/ | ok | 200 |  | tak | manifest 192px, ostra |
| Compiler Explorer | https://godbolt.org/ | ok | 200 |  | nie | s2 128px, ostra |
| Postman | https://web.postman.co/ | zablokowane | 403 | https://identity.getpostman.com/login?continue=https%3A%2F%2Fweb.postman.co%2F&utm_referrer=https%3A%2F%2Fweb.postman.co%2F | nie | favicon 32px, litera |
| Vercel | https://vercel.com/dashboard | ok | 200 | https://vercel.com/login?next=%2Fdashboard | tak | manifest 192px, ostra |
| Cloudflare | https://dash.cloudflare.com/ | zablokowane | 403 |  | nie | apple 180px, ostra |
| Hugging Face | https://huggingface.co/ | ok | 200 |  | nie | s2 256px, ostra |
| Google Maps | https://www.google.com/maps | ok | 200 |  | tak | manifest 192px, ostra |
| Google Translate | https://translate.google.com/ | ok | 200 |  | tak | manifest 64px, kafelek |
| DeepL | https://www.deepl.com/translator | ok | 200 | https://www.deepl.com/pl/translator | nie | apple 180px, ostra |
| OpenStreetMap | https://www.openstreetmap.org/ | ok | 200 |  | tak | manifest 192px, ostra |
| Windy | https://www.windy.com/ | ok | 200 |  | tak | manifest 192px, ostra |
| Speedtest | https://www.speedtest.net/ | ok | 200 |  | tak | manifest 192px, ostra |
| Desmos | https://www.desmos.com/calculator | ok | 200 | https://www.desmos.com/calculator?lang=pl | tak | manifest 192px, ostra |
| GeoGebra | https://www.geogebra.org/calculator | ok | 200 |  | tak | manifest 192px, ostra |
| WeTransfer | https://wetransfer.com/ | ok | 200 |  | nie | apple 180px, ostra |
| Smallpdf | https://smallpdf.com/ | ok | 200 |  | nie | apple 180px, ostra |
| iLovePDF | https://www.ilovepdf.com/ | ok | 200 |  | nie | apple 114px, ostra |
| Bitwarden | https://vault.bitwarden.com/ | ok | 200 |  | tak | manifest 192px, ostra |
| Proton Pass | https://pass.proton.me/ | ok | 200 |  | tak | manifest 192px, ostra |
| Jakdojade | https://jakdojade.pl/ | ok | 200 |  | tak | manifest 192px, ostra |

## Rezerwa (do podmiany błędnych pozycji)

| Nazwa | Adres | Wynik | HTTP | Przekierowanie | Manifest PWA | Ikona |
|---|---|---|---|---|---|---|
| Google Chat | https://chat.google.com/ | ok | 200 | https://accounts.google.com/v3/signin/identifier?continue=https://chat.google.com/&emr=1&osid=1&passive=1209600&service=dynamite&flowName=GlifWebSignIn&flowEntry=ServiceLogin&dsh=S1983413096:1794499062972651 | nie | s2 32px, litera |
| Element | https://app.element.io/ | ok | 200 |  | tak | manifest 180px, ostra |
| Tumblr | https://www.tumblr.com/ | ok | 200 |  | tak | manifest 192px, ostra |
| Yahoo Mail | https://mail.yahoo.com/ | ok | 200 |  | nie | favicon 48px, kafelek |
| Fastmail | https://app.fastmail.com/ | ok | 200 |  | tak | manifest 512px, ostra |
| Medium | https://medium.com/ | zablokowane | 403 |  | nie | s2 180px, ostra |
| Substack | https://substack.com/ | zablokowane | 403 |  | nie | s2 180px, ostra |
| Feedly | https://feedly.com/ | ok | 200 |  | nie | apple 144px, ostra |
| Inoreader | https://www.inoreader.com/ | ok | 200 |  | tak | manifest 192px, ostra |
| Google News | https://news.google.com/ | ok | 200 | https://news.google.com/home?hl=pl&gl=PL&ceid=PL:pl | tak | manifest 192px, ostra |
| Le Chat | https://chat.mistral.ai/ | zablokowane | 403 |  | nie | s2 180px, ostra |
| Grok | https://grok.com/ | ok | 200 |  | tak | manifest 512px, ostra |
| DeepSeek | https://chat.deepseek.com/ | ok | 202 |  | nie | s2 180px, ostra |
| Revolut | https://app.revolut.com/ | zablokowane | 403 |  | nie | ddg 32px, litera |
| Wise | https://wise.com/ | ok | 200 |  | nie | apple 180px, ostra |
| Microsoft To Do | https://to-do.office.com/ | ok | 200 | https://to-do.office.com/tasks/ | nie | s2 180px, ostra |
| Box | https://app.box.com/ | ok | 200 | https://account.box.com/login | nie | s2 180px, ostra |
| MEGA | https://mega.nz/ | ok | 200 |  | nie | apple 180px, ostra |
| Proton Calendar | https://calendar.proton.me/ | ok | 200 |  | tak | manifest 192px, ostra |
| iCloud | https://www.icloud.com/ | ok | 200 |  | nie | s2 180px, ostra |
| Evernote | https://www.evernote.com/client/web | ok | 200 | https://accounts.evernote.com/login | nie | apple 180px, ostra |
| TickTick | https://ticktick.com/webapp | ok | 200 | https://ticktick.com/ | nie | apple 180px, ostra |
| Airtable | https://airtable.com/ | ok | 200 | https://www.airtable.com/ | nie | s2 48px, kafelek |
| Coda | https://coda.io/ | ok | 200 |  | tak | manifest 192px, ostra |
| Calendly | https://calendly.com/ | ok | 200 |  | tak | manifest 192px, ostra |
| Khan Academy | https://www.khanacademy.org/ | ok | 200 |  | nie | s2 144px, ostra |
| Quizlet | https://quizlet.com/ | zablokowane | 403 |  | nie | s2 180px, ostra |
| Coursera | https://www.coursera.org/ | ok | 200 |  | nie | apple 180px, ostra |
| Udemy | https://www.udemy.com/ | zablokowane | 403 |  | nie | s2 180px, ostra |
| Kahoot! | https://create.kahoot.it/ | ok | 200 |  | tak | apple 32px, litera |
| Kick | https://kick.com/ | ok | 200 |  | nie | apple 180px, ostra |
| Dailymotion | https://www.dailymotion.com/ | ok | 200 |  | tak | apple 192px, ostra |
| Pluto TV | https://pluto.tv/ | ok | 200 |  | tak | manifest 256px, ostra |
| Loom | https://www.loom.com/ | ok | 200 |  | nie | apple 180px, ostra |
| Storytel | https://www.storytel.com/ | ok | 200 | https://www.storytel.com/store-select | tak | manifest 192px, ostra |
| Mixcloud | https://www.mixcloud.com/ | ok | 200 |  | nie | apple 152px, ostra |
| Genius | https://genius.com/ | zablokowane | 403 |  | nie | s2 180px, ostra |
| BandLab | https://www.bandlab.com/ | ok | 200 |  | tak | manifest 192px, ostra |
| Unsplash | https://unsplash.com/ | zablokowane | 401 | https://unsplash.com/.within.website?redir=%2F | nie | apple 180px, ostra |
| Behance | https://www.behance.net/ | zablokowane | 403 |  | nie | apple 144px, ostra |
| Replit | https://replit.com/ | ok | 200 |  | tak | manifest 192px, ostra |
| DevDocs | https://devdocs.io/ | ok | 200 |  | tak | manifest 192px, ostra |
| Hoppscotch | https://hoppscotch.io/ | ok | 200 |  | tak | manifest 192px, ostra |
| Supabase | https://supabase.com/dashboard | ok | 200 | https://supabase.com/dashboard/org | tak | manifest 192px, ostra |
| WolframAlpha | https://www.wolframalpha.com/ | ok | 200 |  | nie | s2 64px, kafelek |
| Toggl Track | https://track.toggl.com/ | ok | 200 |  | nie | s2 256px, ostra |
| PairDrop | https://pairdrop.net/ | ok | 200 |  | tak | manifest 192px, ostra |
| Internet Archive | https://archive.org/ | ok | 200 |  | nie | apple 180px, ostra |
| Google Earth | https://earth.google.com/web/ | ok | 200 |  | nie | s2 48px, kafelek |
