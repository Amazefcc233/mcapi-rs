FROM ubuntu:20.04
ENV HTTP_HOST=127.0.0.1:8080
EXPOSE 8080
WORKDIR /app
COPY ./static ./static
COPY ./mcapi-rs /bin/mcapi-rs
CMD ["/bin/mcapi-rs"]
