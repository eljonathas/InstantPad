using System.Net;
using System.Net.Http;
using System.Text;

namespace InstantPad.Tests.Updates;

/// <summary>Answers requests from a table of URLs, with 404 for anything else.</summary>
internal sealed class StubHttpHandler : HttpMessageHandler
{
    private readonly Dictionary<string, Func<HttpResponseMessage>> _responses = [];

    public List<HttpRequestMessage> Requests { get; } = [];

    public StubHttpHandler Respond(string url, string body, HttpStatusCode status = HttpStatusCode.OK) =>
        Respond(url, Encoding.UTF8.GetBytes(body), status);

    public StubHttpHandler Respond(string url, byte[] body, HttpStatusCode status = HttpStatusCode.OK)
    {
        _responses[url] = () => new HttpResponseMessage(status) { Content = new ByteArrayContent(body) };
        return this;
    }

    public StubHttpHandler Fail(string url)
    {
        _responses[url] = () => throw new HttpRequestException("network down");
        return this;
    }

    protected override Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken cancellationToken)
    {
        Requests.Add(request);
        var response = _responses.TryGetValue(request.RequestUri!.ToString(), out var factory)
            ? factory()
            : new HttpResponseMessage(HttpStatusCode.NotFound);
        return Task.FromResult(response);
    }
}
