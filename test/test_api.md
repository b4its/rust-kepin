### API Testing
-   Register
```bash
curl -X POST http://localhost:8000/api/auth/register \
     -H "Content-Type: application/json" \
     -d '{
       "email": "kepin@address.com",
       "name": "KePin",
       "password": "kepin123"
     }'
```

-   Login
```bash
curl -v -X POST http://localhost:8000/api/auth/login \
     -H "Content-Type: application/json" \
     -d '{
       "email": "kepin@address.com",
       "password": "kepin123"
     }'
```

-   Logout
```bash
curl -X POST http://localhost:8000/api/auth/logout
```