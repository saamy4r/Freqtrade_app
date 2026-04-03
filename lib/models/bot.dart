class Bot {
  final String id;
  final String name;
  final String url;
  final String username;
  final String password;

  Bot({
    required this.id,
    required this.name,
    required this.url,
    required this.username,
    required this.password,
  });

  Map<String, dynamic> toJson() {
    return {
      'id': id,
      'name': name,
      'url': url,
      'username': username,
      'password': password,
    };
  }

  factory Bot.fromJson(Map<String, dynamic> json) {
    return Bot(
      id: json['id'],
      name: json['name'],
      url: json['url'],
      username: json['username'],
      password: json['password'],
    );
  }
}
