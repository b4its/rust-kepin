### API Testing
-   Register
```bash
curl -X POST http://localhost:8000/api/v1/auth/register \
     -H "Content-Type: application/json" \
     -d '{
       "email": "kepin@address.com",
       "name": "KePin",
       "password": "kepin123"
     }'
```

-   Login
```bash
curl -v -X POST http://localhost:8000/api/v1/auth/login \
     -H "Content-Type: application/json" \
     -d '{
       "email": "kepin@address.com",
       "password": "kepin123"
     }'
```

-   Logout
```bash
curl -X POST http://localhost:8000/api/v1/auth/logout
```

-   Media Checked
```bash
curl -I http://localhost:8000/public/6942b4ce0591cd64c12de9c1/images/20251218_012821_images.jpg
```
