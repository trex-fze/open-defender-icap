CREATE DATABASE defender_admin;
CREATE DATABASE defender_policy;
GRANT ALL PRIVILEGES ON DATABASE defender_admin TO defender;
GRANT ALL PRIVILEGES ON DATABASE defender_policy TO defender;
ALTER DATABASE defender_admin SET timezone TO 'Asia/Dubai';
ALTER DATABASE defender_policy SET timezone TO 'Asia/Dubai';
ALTER ROLE defender SET timezone TO 'Asia/Dubai';
