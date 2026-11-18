-- Die Weiterleitung per SMS gibt es nicht mehr. Übrig bleiben WhatsApp, Telegram und E-Mail.
alter table notification_rules
    drop column sms,
    drop column sms_number;

delete from notification_log where channel = 'sms';
alter table notification_log drop constraint notification_log_channel_check;
alter table notification_log
    add constraint notification_log_channel_check check (channel in ('whatsapp', 'telegram', 'email'));
