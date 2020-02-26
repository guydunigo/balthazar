const j = await Jobs.at(Jobs.address)
(await j.counter()).toNumber()
