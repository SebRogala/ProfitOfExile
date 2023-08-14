<?php

namespace App\HttpAction;

use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\Routing\Annotation\Route;

class BaseVue extends AbstractController
{
    #[Route("/app/",    name: "vue_home",    methods: ["GET"])]
    public function renderBaseTemplate(): Response
    {
        return $this->render('vue.html.twig');
    }
}
