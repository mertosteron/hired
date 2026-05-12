# Hired

Terminal tabanlı bir iş/staj başvuru botu. Şirket websitelerini tarayarak HR email adreslerini bulur, CV ve transkriptini otomatik olarak gönderir.

```
URL listesi gir → Siteleri tara → Email seç → Mesaj yaz → Gönder
```

---

## Ne yapar?

1. Verdiğin şirket URL'lerini tarar (ana sayfa + iletişim/kariyer sayfaları).
2. Bulunan email adreslerini sıralar ve hangisini kullanacağını seçmeni ister.
3. Yazdığın mesajı, CV'ni ve (isteğe bağlı) transkriptini mail ile gönderir.
4. Spama düşmemek için mailler arasında rastgele 1-4 dakika bekler.
5. Gönderim sonuçlarını `send_log_YYYYMMDD.csv` dosyasına kaydeder.

---

## Kurulum

Rust 1.75+ gerekli. [rustup.rs](https://rustup.rs) ile yükleyebilirsin.

```bash
cargo install --path .
```

Sonrasında herhangi bir terminalden `hired` yazarak başlatabilirsin.

Güncelleme için: `cargo install --path . --force`
Kaldırmak için: `cargo uninstall hired`

> `hired` komutu bulunamazsa `~/.cargo/bin` klasörünün PATH'te olduğunu kontrol et:
> ```bash
> export PATH="$HOME/.cargo/bin:$PATH"
> ```

---

## Yapılandırma

```bash
cp config.example.toml config.toml
```

`config.toml` dosyasını aç ve şu alanları doldur:

```toml
default_subject = "Staj Başvurusu — Adın Soyadın"
default_body    = """Merhaba, ..."""
cv_path         = "cv.pdf"
transcript_path = "transkript.pdf"   # boş bırakırsan eklenmez

send_delay_min_secs = 60    # mailler arası minimum bekleme (saniye)
send_delay_max_secs = 240   # mailler arası maksimum bekleme (saniye)
daily_limit         = 50    # oturum başına maksimum mail sayısı
send_window_start   = 8     # kaçta gönderilmeye başlansın (saat, 0-23)
send_window_end     = 22    # kaçta durulsun (saat, 0-23)

[smtp]
server       = "smtp.gmail.com"
port         = 587           # 587 = STARTTLS, 465 = TLS
username     = "sen@gmail.com"
password     = "uygulama-sifresi"   # Google App Password
from_address = "sen@gmail.com"
from_name    = "Adın Soyadın"
```

> **Gmail kullanıyorsan:** normal şifren çalışmaz. Google hesabında
> [App Password](https://support.google.com/accounts/answer/185833) oluştur ve onu kullan.

`config.toml` ve CV dosyaları **çalışma dizininde** olmalı. Uygulamayı bu klasörden başlat.

---

## Kullanım

```bash
hired
```

### Ekranlar

| Ekran | Tuşlar |
|---|---|
| **URLs** | URL'leri alt alta yaz/yapıştır. `F2` taramayı başlatır. `Ctrl+L` → `urls.txt`'yi yükler. `Ctrl+Q` çıkış. |
| **Scraping** | Otomatik ilerler, beklenir. |
| **Review** | `↑/↓` site seç, `←/→` email adayı değiştir, `Space` dahil et/çıkar, `F2` devam et. `Esc` geri. |
| **Compose** | `Tab/BackTab` alanlar arası geçiş (konu / mesaj / CV / transkript). `F2` gönderimi başlatır. `Esc` geri. |
| **Sending** | Otomatik ilerler. Mailler arası rastgele bekleme uygulanır. |
| **Done** | Sonuçlar gösterilir, `send_log_YYYYMMDD.csv` oluşturulur. `q` veya `Esc` ile çık. |

---

## Nasıl çalışır?

**Email tarama (5 aşamalı):**
1. Ana sayfa HTML metni — regex ile email arama
2. `mailto:` href linkleri
3. İletişim/kariyer/hakkımızda alt sayfaları (8'e kadar)
4. Tüm HTML element attribute'ları (`data-email` gibi gizli alanlar)
5. `<script>` tag içerikleri (JS ile yüklenen siteler)

Bulunan adresler öncelik sırasına göre sıralanır: `hr@`, `kariyer@`, `jobs@` gibi adresler öne gelir. `noreply@` ve spam adresler elenir.

**Spam koruması:**
- Mailler arası rastgele gecikme (varsayılan: 1–4 dakika)
- Günlük gönderim limiti (varsayılan: 50)
- Sadece belirtilen saat aralığında gönderim (varsayılan: 08:00–22:00)

---

## Proje yapısı

```
src/
├── main.rs     — giriş noktası, terminal kurulumu
├── app.rs      — uygulama state makinesi ve event döngüsü
├── ui.rs       — ratatui ile her ekranın çizimi
├── scraper.rs  — 5 aşamalı email tarayıcı
├── mailer.rs   — SMTP gönderici (CV + transkript eki)
├── config.rs   — TOML config
└── error.rs    — hata tipleri
```
