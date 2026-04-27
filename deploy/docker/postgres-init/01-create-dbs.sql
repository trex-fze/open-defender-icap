CREATE DATABASE defender_admin;
CREATE DATABASE defender_policy;
GRANT ALL PRIVILEGES ON DATABASE defender_admin TO CURRENT_USER;
GRANT ALL PRIVILEGES ON DATABASE defender_policy TO CURRENT_USER;
ALTER DATABASE defender_admin SET timezone TO 'Asia/Dubai';
ALTER DATABASE defender_policy SET timezone TO 'Asia/Dubai';
ALTER ROLE CURRENT_USER SET timezone TO 'Asia/Dubai';
