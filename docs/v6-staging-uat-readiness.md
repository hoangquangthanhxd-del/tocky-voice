# TOCKY V6 — staging UAT readiness

Tài liệu này là kế hoạch triển khai và checklist UAT. Thay đổi hiện tại chưa deploy staging
hoặc production.

## Contract đã pin

- Backend source of truth: immutable Supabase snapshot revision `2`, fingerprint
  `sha256:ade79022f4cdc0a5dc1ea3d98f2f3e22c842f2e2bb7df327aa4a53e1a79ee123`.
- Effective dictionary: 1.316 canonical; 568 alias trong artifact đã audit.
- Provider projection: 79 canonical, deterministic, deduplicated, hard cap 100.
- Customer/garage/place: local normalization only; không gửi provider.
- Mỗi session pin một `Arc<CompiledVocabulary>`; refresh chỉ áp dụng cho session kế tiếp.
- Cache TOCKY: `ptap-vocabulary-cache.json`, schema version + SHA-256 checksum, không chứa
  API key/token/audio. File nằm trong Tauri app-data của user (Windows: thư mục app-data của
  `pro.mecode.tockyvoice`), JSON envelope gồm `schema_version`, `checksum`, `snapshot`.
  Sync hợp lệ ghi đè cache; cache lỗi, rỗng, collision hoặc mismatch đều bị từ chối.
- Cache PTAP web: localStorage key `ptap:dictation-vocabulary-cache:v1`, chỉ chứa snapshot
  đã validate. Mỗi prewarm/start thử refresh backend trước; khi offline mới dùng last-known-good.
  Cache không chứa API key STT, Supabase access token hoặc raw audio.
- Bridge: `ws://127.0.0.1:17891/bridge`, protocol 1, request ID + nonce; không bind LAN;
  chỉ chấp nhận Origin loopback dev hoặc `*.ptap-next-staging.pages.dev`.

## Kết quả performance local

Đo bằng `cargo run --quiet --example terminology_perf -- <automotive-vocabulary-v6.json>`
trên snapshot thật:

| Gate | Kết quả |
| --- | ---: |
| Đọc file | 5,534 ms |
| Parse JSON 1.316 mục | 3,174 ms |
| Dựng alias index | 17,474 ms |
| Normalize lần đầu | 0,255 ms |
| Normalize 10.000 lần | 1.827,483 ms (~0,183 ms/lần) |
| Dựng provider projection | 4,140 ms; 79 mục |

## Trình tự deploy staging đề xuất

1. Backup schema/data staging và ghi lại revision/fingerprint active hiện tại.
2. Deploy migration Supabase trước; xác nhận revision 2, fingerprint, 1.316/79 và RPC auth.
3. Deploy Edge Function dictation; smoke test ticket pin đúng snapshot DB.
4. Deploy PTAP web; xác nhận cache refresh/fallback, PC transport và mobile remote fallback.
5. Cài TOCKY 0.5 trên máy UAT Windows; không phát hành production updater/release.
6. Chạy checklist dưới đây, lưu evidence và sign-off. Nếu fail, rollback web/Edge và đặt lại
   `voice_dictation_settings.active_vocabulary_revision` về revision đã backup.

## Checklist UAT staging

- [ ] User đã đăng nhập nhận đúng revision/fingerprint; anon bị từ chối.
- [ ] PC: PTAP nhận `hello`, `prepared`, `recording_started`, rồi đúng một final envelope.
- [ ] Kết quả PC chứa raw + normalized text + revision/fingerprint; mismatch fail rõ ràng.
- [ ] Offline sau một lần sync dùng cache hợp lệ; cache corrupt/rỗng không được dùng.
- [ ] Refresh giữa session không đổi revision của session đang chạy.
- [ ] `PT2`, `MD3`, `DENSO TQ`, `NSK JAPAN`, hai câu cao su cân bằng, `MẶT GƯƠNG` đúng.
- [ ] `950` đơn lẻ không map; sau product context mới map `TOWNER 950`.
- [ ] SKU/code run không đi qua normalizer thuật ngữ.
- [ ] Soniox/Deepgram/AssemblyAI chỉ nhận projection 79, không có customer/garage/place.
- [ ] Mobile không thử loopback PC; remote STT chỉ chạy khi consent là literal `true`.
- [ ] Cancel/stop/disconnect/mic denied/provider timeout/empty transcript trả lỗi có hành động.
- [ ] Privacy log không có token, API key, audio hoặc toàn bộ transcript nhạy cảm.
- [ ] Regression: standalone TOCKY vẫn ghi/dán bình thường; PTAP PC/mobile flows khác không đổi.

Production deployment chỉ được cân nhắc sau khi toàn bộ checkbox có evidence và chủ sản phẩm
ký UAT staging.
