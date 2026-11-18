-- Weitergeleitet wird nur noch per E-Mail: WhatsApp und Telegram gibt es nicht mehr.
--
-- Wer die automatische Weiterleitung nur über einen der anderen Dienste eingeschaltet hatte, bekommt nichts mehr, bis die
-- Regel mit einer E-Mail-Adresse neu eingeschaltet wird. Der Hauptschalter gilt jetzt für die E-Mail allein, die eigene
-- Spalte `email` entfällt.
update notification_rules set enabled = enabled and email;

alter table notification_rules
    drop column whatsapp,
    drop column whatsapp_number,
    drop column whatsapp_key,
    drop column telegram,
    drop column telegram_chat_id,
    drop column email;

-- Im Protokoll steht nur noch die E-Mail, die Spalte für den Dienst entfällt samt ihrer Einschränkung.
delete from notification_log where channel <> 'email';
alter table notification_log drop column channel;
