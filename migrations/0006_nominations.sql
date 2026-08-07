-- Human-facing names for the things a user picks (issue #34).
--
-- Identity and display are deliberately separate columns. A Media Target's
-- identity is its marker, which never changes; its label is whatever the user
-- called it and may be edited freely. Deriving the label from the identity
-- would mean a card called "Odin SD" could not be renamed without becoming a
-- different target.
--
-- Both are nullable. A target adopted from a marker written by an earlier
-- version has no label until someone gives it one, and showing its identifier
-- is a worse answer than showing nothing but an honest one.

ALTER TABLE media_target ADD COLUMN label TEXT;
ALTER TABLE rom_pack ADD COLUMN title TEXT;
