
class Bot {
  final String id; // Unique ID for each bot
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

  // Method to convert a Bot instance to a Map
  Map<String, dynamic> toJson() {
    return {
      'id': id,
      'name': name,
      'url': url,
      'username': username,
      'password': password,
    };
  }

  // Factory constructor to create a Bot instance from a Map
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