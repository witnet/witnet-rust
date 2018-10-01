### RAD Engine schema

```
                              + Webdriver actions --> Gecko_driver, chrome_driver, ...
                              |
                              |
                + WEB request + RAD.js --> Headless browser: chrome, firefox,
                |             |
                |             |
                |             + Third Pary library --> Pupeteer, selenium, ...
                |
RAD interpreter +
                |
                |
                |
                + API request

```